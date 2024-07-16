use std::io::{self, Write};

use bytes::Bytes;

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

    fn bytes_needed(&mut self, _bytes: &[u8]) -> io::Result<Option<usize>> {
        Ok(Some(0))
    }

    fn parse_bytes(&mut self, _bytes: Bytes) -> io::Result<Self::ProtocolMessage> {
        Ok(())
    }

    fn write_bytes<W>(&mut self, _message: &Self::ProtocolMessage, _writer: W) -> io::Result<()>
    where
        W: Write,
    {
        Ok(())
    }

    fn message_size(&mut self, _message: &Self::ProtocolMessage) -> usize {
        0
    }
}

impl<M> NestedPeerProtocol<M> for UnitProtocol {
    fn received_message(&mut self, _message: &M) {}

    fn sent_message(&mut self, _message: &M) {}
}
