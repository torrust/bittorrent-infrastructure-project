use std::collections::HashSet;
use std::io::{self, Read};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::thread::{self};

use dht::handshaker_trait::HandshakerTrait;
use dht::{DhtBuilder, Router};
use util::bt::{InfoHash, PeerId};

struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            println!("{} - {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

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
    fn connect(&mut self, _: Option<PeerId>, _: InfoHash, addr: SocketAddr) {
        if self.filter.contains(&addr) {
            return;
        }

        self.filter.insert(addr);
        self.count += 1;
        println!("Received new peer {:?}, total unique peers {}", addr, self.count);
    }

    /// Send the given Metadata back to the client.
    fn metadata(&mut self, _: Self::MetadataEnvelope) {}
}

fn main() {
    log::set_logger(&SimpleLogger).unwrap();
    log::set_max_level(log::LevelFilter::max());

    let hash = InfoHash::from_bytes(b"My Unique Info Hash");

    let handshaker = SimpleHandshaker {
        filter: HashSet::new(),
        count: 0,
    };
    let dht = DhtBuilder::with_router(Router::uTorrent)
        .set_source_addr(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 6889)))
        .set_read_only(false)
        .start_mainline(handshaker)
        .unwrap();

    // Spawn a thread to listen to and report events
    let events = dht.events();
    thread::spawn(move || {
        for event in events {
            println!("\nReceived Dht Event {:?}", event);
        }
    });

    // Let the user announce or search on our info hash
    let stdin = io::stdin();
    let stdin_lock = stdin.lock();
    for byte in stdin_lock.bytes() {
        match &[byte.unwrap()] {
            b"a" => dht.search(hash, true),
            b"s" => dht.search(hash, false),
            _ => (),
        }
    }
}
