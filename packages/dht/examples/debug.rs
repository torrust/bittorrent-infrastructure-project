use std::collections::HashSet;
use std::io::Read as _;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Once;

use dht::handshaker_trait::HandshakerTrait;
use dht::{DhtBuilder, Router};
use futures::future::BoxFuture;
use futures::StreamExt;
use tokio::task::JoinSet;
use tracing::level_filters::LevelFilter;
use util::bt::{InfoHash, PeerId};

static INIT: Once = Once::new();

struct SimpleHandshaker {
    filter: HashSet<SocketAddr>,
    count: usize,
}

impl HandshakerTrait for SimpleHandshaker {
    /// Type of stream used to receive connections from.
    type MetadataEnvelope = ();

    /// Unique peer id used to identify ourselves to other peers.
    fn id(&self) -> PeerId {
        [0u8; 20].into()
    }

    /// Advertise port that is being listened on by the handshaker.
    ///
    /// It is important that this is the external port that the peer will be sending data
    /// to. This is relevant if the client employs nat traversal via upnp or other means.
    fn port(&self) -> u16 {
        6889
    }

    /// Initiates a handshake with the given socket address.
    fn connect(&mut self, _: Option<PeerId>, _: InfoHash, addr: SocketAddr) -> BoxFuture<'_, ()> {
        if self.filter.contains(&addr) {
            return Box::pin(std::future::ready(()));
        }

        self.filter.insert(addr);
        self.count += 1;
        println!("Received new peer {:?}, total unique peers {}", addr, self.count);

        Box::pin(std::future::ready(()))
    }

    /// Send the given Metadata back to the client.
    fn metadata(&mut self, (): Self::MetadataEnvelope) {}
}

fn tracing_stderr_init(filter: LevelFilter) {
    let builder = tracing_subscriber::fmt().with_max_level(filter).with_ansi(true);

    builder.pretty().with_file(true).init();

    tracing::info!("Logging initialized");
}

#[tokio::main]
async fn main() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::INFO);
    });

    let mut tasks = JoinSet::new();

    let hash = InfoHash::from_bytes(b"My Unique Info Hash");

    let handshaker = SimpleHandshaker {
        filter: HashSet::new(),
        count: 0,
    };
    let dht = DhtBuilder::with_router(Router::uTorrent)
        .set_source_addr(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 6889)))
        .set_read_only(false)
        .start_mainline(handshaker)
        .await
        .unwrap();

    // Spawn a thread to listen to and report events
    let mut events = dht.events().await;
    tasks.spawn(async move {
        while let Some(event) = events.next().await {
            println!("\nReceived Dht Event {event:?}");
        }
    });

    // Let the user announce or search on our info hash
    let stdin = std::io::stdin();
    let stdin_lock = stdin.lock();
    for byte in stdin_lock.bytes() {
        match &[byte.unwrap()] {
            b"a" => dht.search(hash, true).await,
            b"s" => dht.search(hash, false).await,
            _ => (),
        }
    }
}
