use futures::{Sink, Stream, TryStream};
use thiserror::Error;

use crate::manager::peer_info::PeerInfo;

/// Trait for providing `PeerManager` with necessary message information.
///
/// Any `PeerProtocol` (or plain `Codec`) that wants to be managed by `PeerManager`
/// must ensure that its message type implements this trait to provide the necessary hooks.
pub trait ManagedMessage: std::fmt::Debug {
    /// Retrieves a keep-alive message variant.
    fn keep_alive() -> Self;

    /// Checks whether this message is a keep-alive message.
    fn is_keep_alive(&self) -> bool;
}

//----------------------------------------------------------------------------//

/// Identifier for matching sent messages with received messages.
pub type MessageId = u64;

/// Messages that can be sent to the `PeerManager`.
#[derive(Debug)]
pub enum PeerManagerInputMessage<Peer, Message>
where
    Peer: Sink<std::io::Result<Message>>
        + Stream<Item = std::io::Result<Message>>
        + TryStream<Ok = Message, Error = std::io::Error>
        + std::fmt::Debug
        + Send
        + Unpin
        + 'static,
    Message: ManagedMessage + Send + 'static,
{
    /// Adds a peer to the peer manager.
    AddPeer(PeerInfo, Peer),
    /// Removes a peer from the peer manager.
    RemovePeer(PeerInfo),
    /// Sends a message to a peer.
    SendMessage(PeerInfo, MessageId, Message), // TODO: Support querying for statistics
}

#[derive(Error, Debug)]
pub enum PeerManagerOutputError {
    #[error("Peer Disconnected, but Missing")]
    PeerDisconnectedAndMissing(PeerInfo),

    #[error("Peer Removed, but Missing")]
    PeerRemovedAndMissing(PeerInfo),

    #[error("Peer Errored, but Missing")]
    PeerErrorAndMissing(PeerInfo, Option<Box<Self>>),

    #[error("Error with Peer")]
    PeerError(PeerInfo, std::io::Error),
}

/// Messages that can be received from the `PeerManager`.
#[derive(Debug)]
pub enum PeerManagerOutputMessage<Message> {
    /// Indicates a peer has been added to the peer manager.
    PeerAdded(PeerInfo),
    /// Indicates a peer has been removed from the peer manager.
    PeerRemoved(PeerInfo),
    /// Indicates a message has been sent to the given peer.
    SentMessage(PeerInfo, MessageId),
    /// Indicates a message has been received from a peer.
    ReceivedMessage(PeerInfo, Message),
    /// Indicates a peer has disconnected.
    ///
    /// Same semantics as `PeerRemoved`, but the peer is not returned.
    PeerDisconnect(PeerInfo),
}
