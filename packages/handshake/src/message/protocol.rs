use nom::bytes::complete::take;
use nom::number::complete::u8;
use nom::sequence::tuple;
use nom::IResult;
use tokio::io::{AsyncWrite, AsyncWriteExt as _};

const BT_PROTOCOL: &[u8] = b"BitTorrent protocol";
const BT_PROTOCOL_LEN: u8 = 19;

/// `Protocol` information transmitted as part of the handshake.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Protocol {
    BitTorrent,
    Custom(Vec<u8>),
}

impl Protocol {
    /// Create a `Protocol` from the given bytes.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to construct from bytes.
    pub fn from_bytes(bytes: &[u8]) -> IResult<&[u8], Protocol> {
        parse_protocol(bytes)
    }

    /// Write the `Protocol` out to the given writer.
    ///
    /// # Errors
    ///
    /// It would return an IO Error if unable to write bytes.
    pub async fn write_bytes<W>(&self, writer: &mut W) -> std::io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let (len, bytes) = match self {
            Protocol::BitTorrent => (BT_PROTOCOL_LEN as usize, BT_PROTOCOL),
            Protocol::Custom(prot) => (prot.len(), &prot[..]),
        };

        #[allow(clippy::cast_possible_truncation)]
        writer.write_all(&[len as u8][..]).await?;
        writer.write_all(bytes).await?;

        Ok(())
    }

    /// Write the `Extensions` to the given writer.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to write bytes.
    pub fn write_bytes_sync<W>(&self, writer: &mut W) -> std::io::Result<()>
    where
        W: std::io::Write,
    {
        let (len, bytes) = match self {
            Protocol::BitTorrent => (BT_PROTOCOL_LEN as usize, BT_PROTOCOL),
            Protocol::Custom(prot) => (prot.len(), &prot[..]),
        };

        #[allow(clippy::cast_possible_truncation)]
        writer.write_all(&[len as u8][..])?;
        writer.write_all(bytes)?;

        Ok(())
    }

    /// Get the length of the given protocol (does not include the length byte).
    #[must_use]
    pub fn write_len(&self) -> usize {
        match self {
            Protocol::BitTorrent => BT_PROTOCOL_LEN as usize,
            Protocol::Custom(custom) => custom.len(),
        }
    }
}

fn parse_protocol(bytes: &[u8]) -> IResult<&[u8], Protocol> {
    parse_real_protocol(bytes)
}

fn parse_real_protocol(bytes: &[u8]) -> IResult<&[u8], Protocol> {
    let (remaining, (_length, raw_protocol)) = tuple((u8, take(bytes[0] as usize)))(bytes)?;
    if raw_protocol == BT_PROTOCOL {
        Ok((remaining, Protocol::BitTorrent))
    } else {
        Ok((remaining, Protocol::Custom(raw_protocol.to_vec())))
    }
}

#[allow(dead_code)]
fn parse_raw_protocol(bytes: &[u8]) -> IResult<&[u8], &[u8]> {
    let (remaining, (_length, raw_protocol)) = tuple((u8, take(bytes[0] as usize)))(bytes)?;
    Ok((remaining, raw_protocol))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_bytes_bittorrent() {
        let input = [
            19, b'B', b'i', b't', b'T', b'o', b'r', b'r', b'e', b'n', b't', b' ', b'p', b'r', b'o', b't', b'o', b'c', b'o', b'l',
        ];
        let (_, protocol) = Protocol::from_bytes(&input).unwrap();
        assert_eq!(protocol, Protocol::BitTorrent);
    }

    #[tokio::test]
    async fn test_write_bytes_bittorrent() {
        let protocol = Protocol::BitTorrent;
        let mut buffer = Vec::new();
        let () = protocol.write_bytes(&mut buffer).await.unwrap();
        assert_eq!(
            buffer,
            vec![
                19, b'B', b'i', b't', b'T', b'o', b'r', b'r', b'e', b'n', b't', b' ', b'p', b'r', b'o', b't', b'o', b'c', b'o',
                b'l'
            ]
        );
    }
}
