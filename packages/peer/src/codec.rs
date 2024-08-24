//! Codecs operating over `PeerProtocol`s.

use bytes::{BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use crate::protocol::PeerProtocol;

/// Codec operating over some `PeerProtocol`.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct PeerProtocolCodec<P> {
    protocol: P,
    max_payload: Option<usize>,
}

impl<P> PeerProtocolCodec<P> {
    /// Create a new `PeerProtocolCodec`.
    ///
    /// It is strongly recommended to use `PeerProtocolCodec::with_max_payload`
    /// instead of this function, as this function will not enforce a limit on
    /// received payload length.
    pub fn new(protocol: P) -> PeerProtocolCodec<P> {
        PeerProtocolCodec {
            protocol,
            max_payload: None,
        }
    }

    /// Create a new `PeerProtocolCodec` which will yield an error if
    /// receiving a payload larger than the specified `max_payload`.
    pub fn with_max_payload(protocol: P, max_payload: usize) -> PeerProtocolCodec<P> {
        PeerProtocolCodec {
            protocol,
            max_payload: Some(max_payload),
        }
    }
}

impl<P> Decoder for PeerProtocolCodec<P>
where
    P: PeerProtocol,
    <P as PeerProtocol>::ProtocolMessageError: std::error::Error + Send + Sync + 'static,
{
    type Item = P::ProtocolMessage;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let bytes_needed = self.protocol.bytes_needed(src)?;

        let Some(bytes_needed) = bytes_needed else {
            return Ok(None);
        };

        if let Some(max_payload) = self.max_payload {
            if bytes_needed > max_payload {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "PeerProtocolCodec Enforced Maximum Payload Check For Peer",
                ));
            }
        };

        let bytes = if bytes_needed <= src.len() {
            src.split_to(bytes_needed).freeze()
        } else {
            return Ok(None);
        };

        match self.protocol.parse_bytes(&bytes) {
            Ok(item) => item.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)),
            Err(err) => Err(err),
        }
        .map(Some)
    }
}

impl<P> Encoder<std::io::Result<P::ProtocolMessage>> for PeerProtocolCodec<P>
where
    P: PeerProtocol,
{
    type Error = std::io::Error;

    fn encode(&mut self, item: std::io::Result<P::ProtocolMessage>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let message = Ok(item?);

        let size = self.protocol.message_size(&message)?;

        dst.reserve(size);

        let _ = self.protocol.write_bytes(&message, dst.writer())?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use tokio_util::codec::Decoder as _;

    use super::PeerProtocolCodec;
    use crate::protocol::PeerProtocol;

    struct ConsumeProtocol;

    impl PeerProtocol for ConsumeProtocol {
        type ProtocolMessage = ();
        type ProtocolMessageError = std::io::Error;

        fn bytes_needed(&mut self, bytes: &[u8]) -> std::io::Result<Option<usize>> {
            Ok(Some(bytes.len()))
        }

        fn parse_bytes(&mut self, _: &[u8]) -> std::io::Result<Result<Self::ProtocolMessage, Self::ProtocolMessageError>> {
            Ok(Ok(()))
        }

        fn write_bytes<W>(
            &mut self,
            _: &Result<Self::ProtocolMessage, Self::ProtocolMessageError>,
            _: W,
        ) -> std::io::Result<usize>
        where
            W: std::io::Write,
        {
            Ok(0)
        }

        fn message_size(&mut self, _: &Result<Self::ProtocolMessage, Self::ProtocolMessageError>) -> std::io::Result<usize> {
            Ok(0)
        }
    }

    #[test]
    fn positive_parse_at_max_payload() {
        let mut codec = PeerProtocolCodec::with_max_payload(ConsumeProtocol, 100);
        let mut bytes = BytesMut::with_capacity(100);

        bytes.extend_from_slice(&[0u8; 100]);

        let () = codec.decode(&mut bytes).unwrap().unwrap();

        assert_eq!(bytes.len(), 0);
    }

    #[test]
    fn negative_parse_above_max_payload() {
        let mut codec = PeerProtocolCodec::with_max_payload(ConsumeProtocol, 100);
        let mut bytes = BytesMut::with_capacity(200);

        bytes.extend_from_slice(&[0u8; 200]);

        assert!(codec.decode(&mut bytes).is_err());
        assert_eq!(bytes.len(), 200);
    }
}
