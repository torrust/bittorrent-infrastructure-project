extern crate bytes;
extern crate futures;
extern crate util;
#[macro_use]
extern crate nom;
extern crate rand;
extern crate tokio_core;
#[macro_use]
extern crate tokio_io;
extern crate tokio_timer;

mod bittorrent;
mod discovery;
mod filter;
mod handshake;
mod local_addr;
mod message;
mod transport;

pub use discovery::DiscoveryInfo;
pub use filter::{FilterDecision, HandshakeFilter, HandshakeFilters};
pub use handshake::config::HandshakerConfig;
pub use handshake::handshaker::{Handshaker, HandshakerBuilder, HandshakerSink, HandshakerStream};
pub use local_addr::LocalAddr;
pub use message::complete::CompleteMessage;
pub use message::extensions::{Extension, Extensions};
pub use message::initiate::InitiateMessage;
pub use message::protocol::Protocol;
pub use transport::Transport;

/// Built in objects implementing `Transport`.
pub mod transports {
    pub use crate::transport::{TcpListenerStream, TcpTransport};
}

pub use util::bt::{InfoHash, PeerId};
