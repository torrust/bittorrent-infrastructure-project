use std::collections::{HashMap, HashSet};
use std::net::{SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::{Arc, Mutex};

use futures::future::Either;
use futures::sink::Sink;
use futures::stream::Stream;
use futures::sync::mpsc::{self, SendError, UnboundedReceiver, UnboundedSender};
use futures::{Poll, StartSend};
use handshake::{DiscoveryInfo, InitiateMessage};
use util::bt::{InfoHash, PeerId};
use util::trans::{LocallyShuffledIds, TransactionIds};
use utracker::announce::{AnnounceEvent, AnnounceRequest, AnnounceResponse};
use utracker::contact::{CompactPeers, CompactPeersV4, CompactPeersV6};
use utracker::scrape::{ScrapeRequest, ScrapeResponse, ScrapeStats};
use utracker::{ClientMetadata, ServerHandler, ServerResult};

const NUM_PEERS_RETURNED: usize = 20;

#[derive(Clone)]
pub struct MockTrackerHandler {
    inner: Arc<Mutex<InnerMockTrackerHandler>>,
}

struct InnerMockTrackerHandler {
    cids: HashSet<u64>,
    cid_generator: LocallyShuffledIds<u64>,
    peers_map: HashMap<InfoHash, HashSet<SocketAddr>>,
}

impl MockTrackerHandler {
    #[allow(dead_code)]
    pub fn new() -> MockTrackerHandler {
        MockTrackerHandler {
            inner: Arc::new(Mutex::new(InnerMockTrackerHandler {
                cids: HashSet::new(),
                cid_generator: LocallyShuffledIds::<u64>::new(),
                peers_map: HashMap::new(),
            })),
        }
    }

    #[allow(dead_code)]
    pub fn num_active_connect_ids(&self) -> usize {
        self.inner.lock().unwrap().cids.len()
    }
}

impl ServerHandler for MockTrackerHandler {
    fn connect<R>(&mut self, _: SocketAddr, result: R)
    where
        R: for<'a> FnOnce(ServerResult<'a, u64>),
    {
        let mut inner_lock = self.inner.lock().unwrap();

        let cid = inner_lock.cid_generator.generate();
        inner_lock.cids.insert(cid);

        result(Ok(cid));
    }

    fn announce<'b, R>(&mut self, addr: SocketAddr, id: u64, req: &AnnounceRequest<'b>, result: R)
    where
        R: for<'a> FnOnce(ServerResult<'a, AnnounceResponse<'a>>),
    {
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

            result(Ok(AnnounceResponse::new(
                1800,
                peers.len().try_into().unwrap(),
                peers.len().try_into().unwrap(),
                compact_peers,
            )));
        } else {
            result(Err("Connection ID Is Invalid"));
        }
    }

    fn scrape<'b, R>(&mut self, _: SocketAddr, id: u64, req: &ScrapeRequest<'b>, result: R)
    where
        R: for<'a> FnOnce(ServerResult<'a, ScrapeResponse<'a>>),
    {
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

            result(Ok(response));
        } else {
            result(Err("Connection ID Is Invalid"));
        }
    }
}

//----------------------------------------------------------------------------//

#[allow(dead_code)]
pub fn handshaker() -> (MockHandshakerSink, MockHandshakerStream) {
    let (send, recv) = mpsc::unbounded();

    (MockHandshakerSink { send }, MockHandshakerStream { recv })
}

#[derive(Clone)]
pub struct MockHandshakerSink {
    send: UnboundedSender<Either<InitiateMessage, ClientMetadata>>,
}

pub struct MockHandshakerStream {
    recv: UnboundedReceiver<Either<InitiateMessage, ClientMetadata>>,
}

impl DiscoveryInfo for MockHandshakerSink {
    fn port(&self) -> u16 {
        6969
    }

    fn peer_id(&self) -> PeerId {
        [0u8; 20].into()
    }
}

impl Sink for MockHandshakerSink {
    type SinkItem = Either<InitiateMessage, ClientMetadata>;
    type SinkError = SendError<Self::SinkItem>;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        self.send.start_send(item)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.send.poll_complete()
    }
}

impl Stream for MockHandshakerStream {
    type Item = Either<InitiateMessage, ClientMetadata>;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.recv.poll()
    }
}
