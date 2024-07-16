//! Module for peer discovery.

use std::net::SocketAddr;

use handshake::InfoHash;
use metainfo::Metainfo;
use peer::messages::UtMetadataMessage;
use peer::PeerInfo;
use utracker::announce::ClientState;

use crate::ControlMessage;

pub mod error;

mod ut_metadata;

pub use self::ut_metadata::UtMetadataModule;

/// Enumeration of discovery messages that can be sent to a discovery module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IDiscoveryMessage {
    /// Control message.
    Control(ControlMessage),
    /// Find peers and download the metainfo for the `InfoHash`.
    DownloadMetainfo(InfoHash),
    /// Received a `UtMetadata` message.
    ReceivedUtMetadataMessage(PeerInfo, UtMetadataMessage),
}

/// Enumeration of discovery messages that can be received from a discovery module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ODiscoveryMessage {
    /// Send a dht announce for the `InfoHash`.
    SendDhtAnnounce(InfoHash),
    /// Send a udp tracker announce for the `InfoHash`.
    SendUdpTrackerAnnounce(InfoHash, SocketAddr, ClientState),
    /// Send a `UtMetadata` message.
    SendUtMetadataMessage(PeerInfo, UtMetadataMessage),
    /// We have finished downloading the given `Metainfo`.
    DownloadedMetainfo(Metainfo),
}
