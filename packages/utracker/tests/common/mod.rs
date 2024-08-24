use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;

use futures::channel::mpsc;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use futures::{Sink, Stream};
use handshake::DiscoveryInfo;
use tracing::level_filters::LevelFilter;
use tracing::{instrument, Level};
use util::bt::{InfoHash, PeerId};
use util::trans::{LocallyShuffledIds, TransactionIds};
use utracker::announce::{AnnounceEvent, AnnounceRequest, AnnounceResponse};
use utracker::contact::{CompactPeers, CompactPeersV4, CompactPeersV6};
use utracker::scrape::{ScrapeRequest, ScrapeResponse, ScrapeStats};
use utracker::{HandshakerMessage, ServerHandler, ServerResult};

#[allow(dead_code)]
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(1000);

#[allow(dead_code)]
pub const LOOPBACK_IPV4: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0));

const NUM_PEERS_RETURNED: usize = 20;

#[allow(dead_code)]
pub static INIT: Once = Once::new();

#[allow(dead_code)]
#[derive(PartialEq, Eq, Debug)]
pub enum TimeoutResult {
    TimedOut,
    GotResult,
}

#[allow(dead_code)]
pub fn tracing_stderr_init(filter: LevelFilter) {
    let builder = tracing_subscriber::fmt()
        .with_max_level(filter)
        .with_ansi(true)
        .with_writer(std::io::stderr);

    builder.pretty().with_file(true).init();

    tracing::info!("Logging initialized");
}

#[derive(Debug, Clone)]
pub struct MockTrackerHandler {
    inner: Arc<Mutex<InnerMockTrackerHandler>>,
}

#[derive(Debug)]
pub struct InnerMockTrackerHandler {
    cids: HashSet<u64>,
    cid_generator: LocallyShuffledIds<u64>,
    peers_map: HashMap<InfoHash, HashSet<SocketAddr>>,
}

#[allow(dead_code)]
impl MockTrackerHandler {
    #[instrument(skip(), ret(level = Level::TRACE))]
    pub fn new() -> MockTrackerHandler {
        tracing::debug!("new mock handler");

        MockTrackerHandler {
            inner: Arc::new(Mutex::new(InnerMockTrackerHandler {
                cids: HashSet::new(),
                cid_generator: LocallyShuffledIds::<u64>::new(),
                peers_map: HashMap::new(),
            })),
        }
    }

    pub fn num_active_connect_ids(&self) -> usize {
        self.inner.lock().unwrap().cids.len()
    }
}

impl ServerHandler for MockTrackerHandler {
    #[instrument(skip(self), ret(level = Level::TRACE))]
    fn connect(&mut self, addr: SocketAddr) -> Option<ServerResult<'_, u64>> {
        tracing::debug!("mock connect");

        let mut inner_lock = self.inner.lock().unwrap();

        let cid = inner_lock.cid_generator.generate();
        inner_lock.cids.insert(cid);

        Some(Ok(cid))
    }

    #[instrument(skip(self), ret(level = Level::TRACE))]
    fn announce(
        &mut self,
        addr: SocketAddr,
        id: u64,
        req: &AnnounceRequest<'_>,
    ) -> Option<ServerResult<'_, AnnounceResponse<'_>>> {
        tracing::debug!("mock announce");

        let mut inner_lock = self.inner.lock().unwrap();

        if inner_lock.cids.contains(&id) {
            let peers = inner_lock.peers_map.entry(req.info_hash()).or_default();
            // Ignore any source ip directives in the request
            let store_addr = match addr {
                SocketAddr::V4(v4_addr) => SocketAddr::V4(SocketAddrV4::new(*v4_addr.ip(), req.port())),
                SocketAddr::V6(v6_addr) => SocketAddr::V6(SocketAddrV6::new(*v6_addr.ip(), req.port(), 0, 0)),
            };

            // Resolve what to do with the event
            match req.state().event() {
                AnnounceEvent::Started | AnnounceEvent::Completed | AnnounceEvent::None => peers.insert(store_addr),
                AnnounceEvent::Stopped => peers.remove(&store_addr),
            };

            // Check what type of peers the request warrants
            let compact_peers = if req.source_ip().is_ipv4() {
                let mut v4_peers = CompactPeersV4::new();

                for v4_addr in peers
                    .iter()
                    .filter_map(|addr| match addr {
                        SocketAddr::V4(v4_addr) => Some(v4_addr),
                        SocketAddr::V6(_) => None,
                    })
                    .take(NUM_PEERS_RETURNED)
                {
                    v4_peers.insert(*v4_addr);
                }

                CompactPeers::V4(v4_peers)
            } else {
                let mut v6_peers = CompactPeersV6::new();

                for v6_addr in peers
                    .iter()
                    .filter_map(|addr| match addr {
                        SocketAddr::V4(_) => None,
                        SocketAddr::V6(v6_addr) => Some(v6_addr),
                    })
                    .take(NUM_PEERS_RETURNED)
                {
                    v6_peers.insert(*v6_addr);
                }

                CompactPeers::V6(v6_peers)
            };

            Some(Ok(AnnounceResponse::new(
                1800,
                peers.len().try_into().unwrap(),
                peers.len().try_into().unwrap(),
                compact_peers,
            )))
        } else {
            Some(Err("Connection ID Is Invalid"))
        }
    }

    #[instrument(skip(self), ret(level = Level::TRACE))]
    fn scrape(&mut self, _: SocketAddr, id: u64, req: &ScrapeRequest<'_>) -> Option<ServerResult<'_, ScrapeResponse<'_>>> {
        tracing::debug!("mock scrape");

        let mut inner_lock = self.inner.lock().unwrap();

        if inner_lock.cids.contains(&id) {
            let mut response = ScrapeResponse::new();

            for hash in req.iter() {
                let peers = inner_lock.peers_map.entry(hash).or_default();

                response.insert(ScrapeStats::new(
                    peers.len().try_into().unwrap(),
                    0,
                    peers.len().try_into().unwrap(),
                ));
            }

            Some(Ok(response))
        } else {
            Some(Err("Connection ID Is Invalid"))
        }
    }
}

//----------------------------------------------------------------------------//

#[allow(dead_code)]
pub fn handshaker() -> (MockHandshakerSink, MockHandshakerStream) {
    let (send, recv) = mpsc::unbounded();

    (MockHandshakerSink { send }, MockHandshakerStream { recv })
}

#[derive(Debug, Clone)]
pub struct MockHandshakerSink {
    send: mpsc::UnboundedSender<HandshakerMessage>,
}

impl DiscoveryInfo for MockHandshakerSink {
    fn port(&self) -> u16 {
        6969
    }

    fn peer_id(&self) -> PeerId {
        [0u8; 20].into()
    }
}

impl Sink<std::io::Result<HandshakerMessage>> for MockHandshakerSink {
    type Error = std::io::Error;

    #[instrument(skip(self, cx), ret(level = Level::TRACE))]
    fn poll_ready(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        tracing::trace!("polling ready");

        self.send
            .poll_ready(cx)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }

    #[instrument(skip(self), ret(level = Level::TRACE))]
    fn start_send(mut self: std::pin::Pin<&mut Self>, item: std::io::Result<HandshakerMessage>) -> Result<(), Self::Error> {
        tracing::debug!("starting send");

        self.send
            .start_send(item?)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }

    #[instrument(skip(self, cx), ret(level = Level::TRACE))]
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        tracing::trace!("polling flush");

        self.send
            .poll_flush_unpin(cx)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }

    #[instrument(skip(self, cx), ret(level = Level::TRACE))]
    fn poll_close(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        tracing::debug!("polling close");

        self.send
            .poll_close_unpin(cx)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

pub struct MockHandshakerStream {
    recv: mpsc::UnboundedReceiver<HandshakerMessage>,
}

impl Stream for MockHandshakerStream {
    type Item = std::io::Result<HandshakerMessage>;

    #[instrument(skip(self, cx), ret(level = Level::TRACE))]
    fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
        tracing::trace!("polling next");

        self.recv.poll_next_unpin(cx).map(|maybe| maybe.map(Ok))
    }
}
