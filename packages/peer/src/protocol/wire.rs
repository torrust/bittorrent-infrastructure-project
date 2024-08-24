use crate::message::{BitsExtensionMessage, ExtendedMessage, PeerWireProtocolMessage, PeerWireProtocolMessageError};
use crate::protocol::{NestedPeerProtocol, PeerProtocol};

/// Protocol for peer wire messages.
#[derive(Debug, Clone)]
pub struct PeerWireProtocol<P>
where
    P: Clone,
{
    ext_protocol: P,
}

impl<P> PeerWireProtocol<P>
where
    P: Clone,
{
    /// Create a new `PeerWireProtocol` with the given extension protocol.
    ///
    /// Important to note that nested protocol should follow the same message length format
    /// as the peer wire protocol. This means it should expect a 4 byte (`u32`) message
    /// length prefix. Nested protocols will NOT have their `bytes_needed` method called.
    pub fn new(ext_protocol: P) -> PeerWireProtocol<P> {
        PeerWireProtocol { ext_protocol }
    }
}

impl<P> PeerProtocol for PeerWireProtocol<P>
where
    P: PeerProtocol + NestedPeerProtocol<ExtendedMessage> + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    type ProtocolMessage = PeerWireProtocolMessage<P>;

    type ProtocolMessageError = PeerWireProtocolMessageError;

    fn bytes_needed(&mut self, bytes: &[u8]) -> std::io::Result<Option<usize>> {
        PeerWireProtocolMessage::<P>::bytes_needed(bytes)
    }

    fn parse_bytes(&mut self, bytes: &[u8]) -> std::io::Result<Result<Self::ProtocolMessage, Self::ProtocolMessageError>> {
        match PeerWireProtocolMessage::parse_bytes(bytes, &mut self.ext_protocol)? {
            PeerWireProtocolMessage::BitsExtension(BitsExtensionMessage::Extended(msg)) => {
                self.ext_protocol.received_message(&msg);

                Ok(Ok(PeerWireProtocolMessage::BitsExtension(BitsExtensionMessage::Extended(
                    msg,
                ))))
            }
            other => Ok(Ok(other)),
        }
    }

    fn write_bytes<W>(
        &mut self,
        item: &Result<Self::ProtocolMessage, Self::ProtocolMessageError>,
        writer: W,
    ) -> std::io::Result<usize>
    where
        W: std::io::Write,
    {
        let message = match item {
            Ok(message) => message,
            Err(err) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, err.clone())),
        };

        let message_bytes_written = message.write_bytes(writer, &mut self.ext_protocol)?;

        let PeerWireProtocolMessage::BitsExtension(BitsExtensionMessage::Extended(extended_message)) = message else {
            return Ok(message_bytes_written);
        };

        Ok(self.ext_protocol.sent_message(extended_message))
    }

    fn message_size(&mut self, item: &Result<Self::ProtocolMessage, Self::ProtocolMessageError>) -> std::io::Result<usize> {
        let message = match item {
            Ok(message) => message,
            Err(err) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, err.clone())),
        };

        message.message_size(&mut self.ext_protocol)
    }
}
