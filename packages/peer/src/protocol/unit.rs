use crate::protocol::{NestedPeerProtocol, PeerProtocol};

/// Unit protocol which will always return a unit if called.
#[allow(clippy::module_name_repetitions)]
#[derive(Default)]
pub struct UnitProtocol;

impl UnitProtocol {
    /// Create a new `UnitProtocol`.
    #[must_use]
    pub fn new() -> UnitProtocol {
        UnitProtocol
    }
}

impl PeerProtocol for UnitProtocol {
    type ProtocolMessage = ();

    type ProtocolMessageError = std::io::Error;

    fn bytes_needed(&mut self, _: &[u8]) -> std::io::Result<Option<usize>> {
        Ok(Some(0))
    }

    fn parse_bytes(&mut self, _: &[u8]) -> std::io::Result<Result<Self::ProtocolMessage, Self::ProtocolMessageError>> {
        Ok(Ok(()))
    }

    fn write_bytes<W>(&mut self, _: &Result<Self::ProtocolMessage, Self::ProtocolMessageError>, _: W) -> std::io::Result<usize>
    where
        W: std::io::Write,
    {
        Ok(0)
    }

    fn message_size(&mut self, _: &Result<Self::ProtocolMessage, Self::ProtocolMessageError>) -> std::io::Result<usize> {
        Ok(0)
    }
}

impl<M> NestedPeerProtocol<M> for UnitProtocol {
    fn received_message(&mut self, _message: &M) -> usize {
        0
    }

    fn sent_message(&mut self, _message: &M) -> usize {
        0
    }
}
