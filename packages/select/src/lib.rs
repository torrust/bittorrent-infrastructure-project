use std::time::Duration;

use metainfo::Metainfo;
use peer::PeerInfo;

pub mod discovery;
pub mod error;
pub mod revelation;

mod extended;
mod uber;

pub use uber::{DiscoveryTrait, IUberMessage, OUberMessage, UberModule, UberModuleBuilder};

pub use crate::extended::{ExtendedListener, ExtendedPeerInfo, IExtendedMessage, OExtendedMessage};

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
