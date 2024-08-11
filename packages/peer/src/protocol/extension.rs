use crate::message::{ExtendedMessage, PeerExtensionProtocolMessage, PeerExtensionProtocolMessageError};
use crate::protocol::{NestedPeerProtocol, PeerProtocol};

/// Protocol for `BEP 10` peer extensions.

#[derive(Debug, Clone)]
pub struct PeerExtensionProtocol<P>
where
    P: Clone,
{
    our_extended_msg: Option<ExtendedMessage>,
    their_extended_msg: Option<ExtendedMessage>,
    custom_protocol: P,
}

impl<P> PeerExtensionProtocol<P>
where
    P: Clone,
{
    /// Create a new `PeerExtensionProtocol` with the given (nested) custom extension protocol.
    ///
    /// Notes for `PeerWireProtocol` apply to this custom extension protocol.
    pub fn new(custom_protocol: P) -> PeerExtensionProtocol<P> {
        PeerExtensionProtocol {
            our_extended_msg: None,
            their_extended_msg: None,
            custom_protocol,
        }
    }
}

impl<P> PeerProtocol for PeerExtensionProtocol<P>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    type ProtocolMessage = PeerExtensionProtocolMessage<P>;
    type ProtocolMessageError = PeerExtensionProtocolMessageError;

    fn bytes_needed(&mut self, bytes: &[u8]) -> std::io::Result<Option<usize>> {
        PeerExtensionProtocolMessage::<P>::bytes_needed(bytes)
    }

    fn parse_bytes(&mut self, bytes: &[u8]) -> std::io::Result<Result<Self::ProtocolMessage, Self::ProtocolMessageError>> {
        match self.our_extended_msg {
            Some(ref extended_msg) => PeerExtensionProtocolMessage::parse_bytes(bytes, extended_msg, &mut self.custom_protocol),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Extension Message Received From Peer Before Extended Message...",
            )),
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

        match self.their_extended_msg {
            Some(ref extended_msg) => Ok(PeerExtensionProtocolMessage::write_bytes(
                message,
                writer,
                extended_msg,
                &mut self.custom_protocol,
            )?),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Extension Message Sent From Us Before Extended Message...",
            )),
        }
    }

    fn message_size(&mut self, item: &Result<Self::ProtocolMessage, Self::ProtocolMessageError>) -> std::io::Result<usize> {
        let message = match item {
            Ok(message) => message,
            Err(err) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, err.clone())),
        };

        message.message_size(&mut self.custom_protocol)
    }
}

impl<P> NestedPeerProtocol<ExtendedMessage> for PeerExtensionProtocol<P>
where
    P: NestedPeerProtocol<ExtendedMessage> + Clone,
{
    fn received_message(&mut self, message: &ExtendedMessage) -> usize {
        self.their_extended_msg = Some(message.clone());

        self.custom_protocol.received_message(message)
    }

    fn sent_message(&mut self, message: &ExtendedMessage) -> usize {
        self.our_extended_msg = Some(message.clone());

        self.custom_protocol.sent_message(message)
    }
}
