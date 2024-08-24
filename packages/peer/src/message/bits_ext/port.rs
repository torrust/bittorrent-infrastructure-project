use std::io::Write as _;

use bytes::{BufMut, Bytes, BytesMut};
use nom::combinator::map;
use nom::number::complete::be_u16;
use nom::IResult;

use crate::message;
use crate::message::bits_ext;

/// Message for notifying a peer of our DHT port.
#[allow(clippy::module_name_repetitions)]
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct PortMessage {
    port: u16,
}

impl PortMessage {
    /// Creates a new `PortMessage`.
    ///
    /// # Parameters
    ///
    /// - `port`: The DHT port number.
    ///
    /// # Returns
    ///
    /// A new `PortMessage` instance.
    #[must_use]
    pub fn new(port: u16) -> PortMessage {
        PortMessage { port }
    }

    /// Parses a byte slice into a `PortMessage`.
    ///
    /// # Parameters
    ///
    /// - `bytes`: The byte slice to parse.
    ///
    /// # Returns
    ///
    /// An `IResult` containing the remaining byte slice and the parsed `PortMessage`.
    ///
    /// # Errors
    ///
    /// This function will return an error if the byte slice cannot be parsed into a `PortMessage`.
    pub fn parse_bytes(bytes: &[u8]) -> IResult<&[u8], PortMessage> {
        map(be_u16, PortMessage::new)(bytes)
    }

    /// Writes the current state of the `PortMessage` as bytes.
    ///
    /// # Parameters
    ///
    /// - `writer`: The writer to which the bytes will be written.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to write the bytes.
    pub fn write_bytes<W>(&self, mut writer: W) -> std::io::Result<usize>
    where
        W: std::io::Write,
    {
        let length_len = message::write_length_id_pair(&mut writer, bits_ext::PORT_MESSAGE_LEN, Some(bits_ext::PORT_MESSAGE_ID))?;

        let mut buf = BytesMut::with_capacity(2);
        let () = buf.put_u16(self.port);
        let () = writer.write_all(&buf)?;

        Ok(length_len + 2)
    }
}
