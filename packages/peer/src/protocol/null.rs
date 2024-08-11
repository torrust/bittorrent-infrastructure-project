use crate::message::NullProtocolMessage;
use crate::protocol::{NestedPeerProtocol, PeerProtocol};

/// Null protocol which will return an error if called.
///
/// This protocol is mainly useful for indicating that you do
/// not want to support any `PeerWireProtocolMessage::ProtExtension`
/// messages.
///
/// Of course, you should make sure that you don't tell peers
/// that you support any extended messages.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Default, Clone)]
pub struct NullProtocol;

impl NullProtocol {
    /// Create a new `NullProtocol`.
    #[must_use]
    pub fn new() -> NullProtocol {
        NullProtocol
    }
}

impl PeerProtocol for NullProtocol {
    type ProtocolMessage = NullProtocolMessage;
    type ProtocolMessageError = std::io::Error;

    fn bytes_needed(&mut self, _: &[u8]) -> std::io::Result<Option<usize>> {
        Ok(Some(0))
    }

    fn parse_bytes(&mut self, _: &[u8]) -> std::io::Result<Result<Self::ProtocolMessage, Self::ProtocolMessageError>> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Attempted To Parse Bytes As Null Protocol",
        ))
    }

    fn write_bytes<W>(&mut self, _: &Result<Self::ProtocolMessage, Self::ProtocolMessageError>, _: W) -> std::io::Result<usize>
    where
        W: std::io::Write,
    {
        panic!(
            "bip_peer: NullProtocol::write_bytes Was Called...Wait, How Did You Construct An Instance Of NullProtocolMessage? :)"
        );
    }

    fn message_size(&mut self, _: &Result<Self::ProtocolMessage, Self::ProtocolMessageError>) -> std::io::Result<usize> {
        Ok(0)
    }
}

impl<M> NestedPeerProtocol<M> for NullProtocol {
    fn received_message(&mut self, _message: &M) -> usize {
        0
    }

    fn sent_message(&mut self, _message: &M) -> usize {
        0
    }
}
