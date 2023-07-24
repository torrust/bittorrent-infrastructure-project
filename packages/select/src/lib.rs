extern crate bit_set;
extern crate bytes;
extern crate handshake;
extern crate metainfo;
extern crate peer;
extern crate util;
extern crate utracker;
#[macro_use]
extern crate error_chain;
extern crate futures;
#[macro_use]
extern crate log;
extern crate rand;

#[cfg(test)]
extern crate futures_test;

use std::time::Duration;

use metainfo::Metainfo;
use peer::PeerInfo;

pub mod discovery;
pub mod error;
pub mod revelation;

mod extended;
mod uber;

pub use extended::{ExtendedListener, ExtendedPeerInfo, IExtendedMessage, OExtendedMessage};
pub use uber::{IUberMessage, OUberMessage, UberModule, UberModuleBuilder};

/// Enumeration of control messages most modules will be interested in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlMessage {
    /// Start tracking the given torrent.
    AddTorrent(Metainfo),
    /// Stop tracking the given torrent.
    RemoveTorrent(Metainfo),
    /// Connected to the given peer.
    ///
    /// This message can be sent multiple times, which
    /// is useful if extended peer information changes.
    PeerConnected(PeerInfo),
    /// Disconnected from the given peer.
    PeerDisconnected(PeerInfo),
    /// A span of time has passed.
    ///
    /// This message is vital for certain modules
    /// to function correctly. Subsequent durations
    /// should not be spread too far apart.
    Tick(Duration),
}
