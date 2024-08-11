use nom::bytes::complete::take;
use nom::combinator::map_res;
use nom::sequence::tuple;
use nom::IResult;
use tokio::io::{AsyncWrite, AsyncWriteExt as _};
use util::bt::{self, InfoHash, PeerId};

use crate::message::extensions::{self, Extensions};
use crate::message::protocol::Protocol;

#[allow(clippy::module_name_repetitions)]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HandshakeMessage {
    prot: Protocol,
    ext: Extensions,
    hash: InfoHash,
    pid: PeerId,
}

impl HandshakeMessage {
    /// Create a new `HandshakeMessage` from the given components.
    pub fn from_parts(prot: Protocol, ext: Extensions, hash: InfoHash, pid: PeerId) -> HandshakeMessage {
        if let Protocol::Custom(ref custom) = prot {
            assert!(
                u8::try_from(custom.len()).is_ok(),
                "bip_handshake: Handshake Message With Protocol Length Greater Than {} Found",
                u8::MAX
            );
        }

        HandshakeMessage { prot, ext, hash, pid }
    }

    pub fn from_bytes(bytes: &Vec<u8>) -> IResult<(), HandshakeMessage> {
        parse_remote_handshake(bytes)
    }

    #[allow(dead_code)]
    pub async fn write_bytes<W>(&self, writer: &mut W) -> std::io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        self.prot.write_bytes(writer).await?;
        self.ext.write_bytes(writer).await?;
        writer.write_all(self.hash.as_ref()).await?;

        writer.write_all(self.pid.as_ref()).await?;

        Ok(())
    }

    pub fn write_bytes_sync<W>(&self, writer: &mut W) -> std::io::Result<()>
    where
        W: std::io::Write,
    {
        self.prot.write_bytes_sync(writer)?;
        self.ext.write_bytes_sync(writer)?;
        writer.write_all(self.hash.as_ref())?;

        writer.write_all(self.pid.as_ref())?;

        Ok(())
    }

    pub fn write_len(&self) -> usize {
        #[allow(clippy::cast_possible_truncation)]
        write_len_with_protocol_len(self.prot.write_len() as u8)
    }

    pub fn into_parts(self) -> (Protocol, Extensions, InfoHash, PeerId) {
        (self.prot, self.ext, self.hash, self.pid)
    }
}

pub fn write_len_with_protocol_len(protocol_len: u8) -> usize {
    1 + (protocol_len as usize) + extensions::NUM_EXTENSION_BYTES + bt::INFO_HASH_LEN + bt::PEER_ID_LEN
}

#[allow(clippy::ptr_arg)]
fn parse_remote_handshake(bytes: &Vec<u8>) -> IResult<(), HandshakeMessage> {
    let res = tuple((
        Protocol::from_bytes,
        Extensions::from_bytes,
        parse_remote_hash,
        parse_remote_pid,
    ))(bytes);

    let (_, (prot, ext, hash, pid)) = res.map_err(|e: nom::Err<nom::error::Error<&[u8]>>| e.map_input(|_| ()))?;

    Ok(((), HandshakeMessage::from_parts(prot, ext, hash, pid)))
}

fn parse_remote_hash(bytes: &[u8]) -> IResult<&[u8], InfoHash> {
    map_res(take(bt::INFO_HASH_LEN), |hash: &[u8]| {
        InfoHash::from_hash(hash).map_err(|_| nom::Err::Error((bytes, nom::error::ErrorKind::LengthValue)))
    })(bytes)
}

fn parse_remote_pid(bytes: &[u8]) -> IResult<&[u8], PeerId> {
    map_res(take(bt::PEER_ID_LEN), |pid: &[u8]| {
        PeerId::from_hash(pid).map_err(|_| nom::Err::Error((bytes, nom::error::ErrorKind::LengthValue)))
    })(bytes)
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use util::bt::{self, InfoHash, PeerId};

    use super::HandshakeMessage;
    use crate::message::extensions::{self, Extensions};
    use crate::message::protocol::Protocol;

    fn any_peer_id() -> PeerId {
        [22u8; bt::PEER_ID_LEN].into()
    }

    fn any_info_hash() -> InfoHash {
        [55u8; bt::INFO_HASH_LEN].into()
    }

    fn any_extensions() -> Extensions {
        [255u8; extensions::NUM_EXTENSION_BYTES].into()
    }

    #[test]
    fn positive_decode_zero_bytes_protocol() {
        let mut buffer = Vec::new();

        let exp_protocol = Protocol::Custom(Vec::new());
        let exp_extensions = any_extensions();
        let exp_hash = any_info_hash();
        let exp_pid = any_peer_id();

        let exp_message = HandshakeMessage::from_parts(exp_protocol.clone(), exp_extensions, exp_hash, exp_pid);

        exp_protocol.write_bytes_sync(&mut buffer).unwrap();
        exp_extensions.write_bytes_sync(&mut buffer).unwrap();
        buffer.write_all(exp_hash.as_ref()).unwrap();
        buffer.write_all(exp_pid.as_ref()).unwrap();

        let recv_message = HandshakeMessage::from_bytes(&buffer).unwrap().1;

        assert_eq!(exp_message, recv_message);
    }

    #[test]
    fn positive_many_bytes_protocol() {
        let mut buffer = Vec::new();

        let exp_protocol = Protocol::Custom(b"My Protocol".to_vec());
        let exp_extensions = any_extensions();
        let exp_hash = any_info_hash();
        let exp_pid = any_peer_id();

        let exp_message = HandshakeMessage::from_parts(exp_protocol.clone(), exp_extensions, exp_hash, exp_pid);

        exp_protocol.write_bytes_sync(&mut buffer).unwrap();
        exp_extensions.write_bytes_sync(&mut buffer).unwrap();
        buffer.write_all(exp_hash.as_ref()).unwrap();
        buffer.write_all(exp_pid.as_ref()).unwrap();

        let recv_message = HandshakeMessage::from_bytes(&buffer).unwrap().1;

        assert_eq!(exp_message, recv_message);
    }

    #[test]
    fn positive_bittorrent_protocol() {
        let mut buffer = Vec::new();

        let exp_protocol = Protocol::BitTorrent;
        let exp_extensions = any_extensions();
        let exp_hash = any_info_hash();
        let exp_pid = any_peer_id();

        let exp_message = HandshakeMessage::from_parts(exp_protocol.clone(), exp_extensions, exp_hash, exp_pid);

        exp_protocol.write_bytes_sync(&mut buffer).unwrap();
        exp_extensions.write_bytes_sync(&mut buffer).unwrap();
        buffer.write_all(exp_hash.as_ref()).unwrap();
        buffer.write_all(exp_pid.as_ref()).unwrap();

        let recv_message = HandshakeMessage::from_bytes(&buffer).unwrap().1;

        assert_eq!(exp_message, recv_message);
    }

    #[test]
    #[should_panic(expected = "bip_handshake: Handshake Message With Protocol Length Greater Than 255 Found")]
    fn negative_create_overflow_protocol() {
        let overflow_protocol = Protocol::Custom(vec![0u8; 256]);

        HandshakeMessage::from_parts(overflow_protocol, any_extensions(), any_info_hash(), any_peer_id());
    }
}
