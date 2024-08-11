use std::io::Write as _;

use byteorder::{BigEndian, WriteBytesExt};
use bytes::Bytes;
use nom::bytes::complete::take;
use nom::combinator::{map, map_res};
use nom::number::complete::be_u32;
use nom::sequence::tuple;
use nom::{IResult, Needed};

use crate::message;

/// Message for notifying a peer of a piece that you have.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct HaveMessage {
    piece_index: u32,
}

impl HaveMessage {
    /// Creates a new `HaveMessage`.
    ///
    /// # Parameters
    ///
    /// - `piece_index`: The index of the piece that you have.
    ///
    /// # Returns
    ///
    /// A new `HaveMessage` instance.
    #[must_use]
    pub fn new(piece_index: u32) -> HaveMessage {
        HaveMessage { piece_index }
    }

    /// Parses a byte slice into a `HaveMessage`.
    ///
    /// # Parameters
    ///
    /// - `bytes`: The byte slice to parse.
    ///
    /// # Returns
    ///
    /// An `io::Result` containing the parsed `HaveMessage` or an error if parsing fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the byte slice cannot be parsed into a `HaveMessage`.
    pub fn parse_bytes(bytes: &[u8]) -> std::io::Result<HaveMessage> {
        match parse_have(bytes) {
            Ok((_, msg)) => msg,
            Err(_) => Err(std::io::Error::new(std::io::ErrorKind::Other, "Failed to parse HaveMessage")),
        }
    }

    /// Writes the current state of the `HaveMessage` as bytes.
    ///
    /// # Parameters
    ///
    /// - `writer`: The writer to which the bytes will be written.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to write bytes.
    pub fn write_bytes<W>(&self, mut writer: W) -> std::io::Result<usize>
    where
        W: std::io::Write,
    {
        let id_length = message::write_length_id_pair(&mut writer, message::HAVE_MESSAGE_LEN, Some(message::HAVE_MESSAGE_ID))?;
        let () = writer.write_u32::<BigEndian>(self.piece_index)?;

        Ok(id_length + 4) // + u32
    }

    /// Returns the piece index of the `HaveMessage`.
    ///
    /// # Returns
    ///
    /// The piece index.
    #[must_use]
    pub fn piece_index(&self) -> u32 {
        self.piece_index
    }
}

/// Parses a byte slice into a `HaveMessage`.
///
/// # Parameters
///
/// - `bytes`: The byte slice to parse.
///
/// # Returns
///
/// An `IResult` containing the remaining byte slice and an `io::Result` with the parsed `HaveMessage`.
///
/// # Errors
///
/// This function will return an error if the byte slice cannot be parsed into a `HaveMessage`.
fn parse_have(bytes: &[u8]) -> IResult<&[u8], std::io::Result<HaveMessage>> {
    map(be_u32, |index| Ok(HaveMessage::new(index)))(bytes)
}

// ----------------------------------------------------------------------------//

/// Message for notifying a peer of all of the pieces you have.
///
/// This should be sent immediately after the handshake.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct BitFieldMessage {
    bytes: Bytes,
}

impl BitFieldMessage {
    /// Creates a new `BitFieldMessage`.
    ///
    /// # Parameters
    ///
    /// - `bytes`: The bytes representing the bitfield.
    ///
    /// # Returns
    ///
    /// A new `BitFieldMessage` instance.
    pub fn new(bytes: Bytes) -> BitFieldMessage {
        BitFieldMessage { bytes }
    }

    /// Parses a byte slice into a `BitFieldMessage`.
    ///
    /// # Parameters
    ///
    /// - `bytes`: The byte slice to parse.
    ///
    /// # Returns
    ///
    /// An `io::Result` containing the parsed `BitFieldMessage` or an error if parsing fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the byte slice cannot be parsed into a `BitFieldMessage`.
    pub fn parse_bytes(bytes: &[u8]) -> std::io::Result<BitFieldMessage> {
        let len = bytes.len();
        match parse_bitfield(bytes, len) {
            Ok((_, msg)) => msg,
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to parse BitFieldMessage",
            )),
        }
    }

    /// Writes the current state of the `BitFieldMessage` as bytes.
    ///
    /// # Parameters
    ///
    /// - `writer`: The writer to which the bytes will be written.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to write bytes.
    ///
    /// # Panics
    ///
    /// This function will panic if the length is too long.
    pub fn write_bytes<W>(&self, mut writer: W) -> std::io::Result<usize>
    where
        W: std::io::Write,
    {
        let message_length: u32 = self
            .bytes
            .len()
            .try_into()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Unsupported, e))?;

        let actual_length = message_length + 1; // + Some(message::BITFIELD_MESSAGE_ID);

        let id_length = message::write_length_id_pair(&mut writer, actual_length, Some(message::BITFIELD_MESSAGE_ID))?;
        let () = writer.write_all(&self.bytes)?;

        Ok(id_length + message_length as usize)
    }

    /// Returns the bitfield bytes.
    ///
    /// # Returns
    ///
    /// A slice of the bitfield bytes.
    pub fn bitfield(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns an iterator over the `BitFieldMessage` that yields `HaveMessage`s.
    ///
    /// # Returns
    ///
    /// An iterator over the `BitFieldMessage`.
    #[allow(clippy::iter_without_into_iter)]
    pub fn iter(&self) -> BitFieldIter {
        BitFieldIter::new(self.bytes.clone())
    }
}

/// Parses a byte slice into a `BitFieldMessage`.
///
/// # Parameters
///
/// - `bytes`: The byte slice to parse.
/// - `len`: The length of the bitfield.
///
/// # Returns
///
/// An `IResult` containing the remaining byte slice and an `io::Result` with the parsed `BitFieldMessage`.
///
/// # Errors
///
/// This function will return an error if the byte slice cannot be parsed into a `BitFieldMessage`.
fn parse_bitfield(bytes: &[u8], len: usize) -> IResult<&[u8], std::io::Result<BitFieldMessage>> {
    if bytes.len() >= len {
        Ok((
            &bytes[len..],
            Ok(BitFieldMessage {
                bytes: Bytes::copy_from_slice(&bytes[..len]),
            }),
        ))
    } else {
        Err(nom::Err::Incomplete(Needed::new(len - bytes.len())))
    }
}

/// Iterator for a `BitFieldMessage` to `HaveMessage`s.
pub struct BitFieldIter {
    bytes: Bytes,
    // TODO: Probably not the best type for indexing bits?
    cur_bit: usize,
}

impl BitFieldIter {
    fn new(bytes: Bytes) -> BitFieldIter {
        BitFieldIter { bytes, cur_bit: 0 }
    }
}

impl Iterator for BitFieldIter {
    type Item = HaveMessage;

    fn next(&mut self) -> Option<HaveMessage> {
        let byte_in_bytes = self.cur_bit / 8;
        let bit_in_byte = self.cur_bit % 8;

        let opt_byte = self.bytes.get(byte_in_bytes).copied();
        opt_byte.and_then(|byte| {
            let have_message = HaveMessage::new(self.cur_bit.try_into().unwrap());
            self.cur_bit += 1;

            if (byte << bit_in_byte) >> 7 == 1 {
                Some(have_message)
            } else {
                self.next()
            }
        })
    }
}

// ----------------------------------------------------------------------------//
/// Message for requesting a block from a peer.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct RequestMessage {
    piece_index: u32,
    block_offset: u32,
    block_length: usize,
}

impl RequestMessage {
    /// Creates a new `RequestMessage`.
    ///
    /// # Parameters
    ///
    /// - `piece_index`: The index of the piece being requested.
    /// - `block_offset`: The offset within the piece.
    /// - `block_length`: The length of the block being requested.
    ///
    /// # Returns
    ///
    /// A new `RequestMessage` instance.
    #[must_use]
    pub fn new(piece_index: u32, block_offset: u32, block_length: usize) -> RequestMessage {
        RequestMessage {
            piece_index,
            block_offset,
            block_length,
        }
    }

    /// Parses a byte slice into a `RequestMessage`.
    ///
    /// # Parameters
    ///
    /// - `bytes`: The byte slice to parse.
    ///
    /// # Returns
    ///
    /// An `io::Result` containing the parsed `RequestMessage` or an error if parsing fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the byte slice cannot be parsed into a `RequestMessage`.
    pub fn parse_bytes(bytes: &[u8]) -> std::io::Result<RequestMessage> {
        match parse_request(bytes) {
            Ok((_, msg)) => msg,
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to parse RequestMessage",
            )),
        }
    }

    /// Writes the current state of the `RequestMessage` as bytes.
    ///
    /// # Parameters
    ///
    /// - `writer`: The writer to which the bytes will be written.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to write bytes.
    ///
    /// # Panics
    ///
    /// This function will panic if the `block_length` is too large.
    pub fn write_bytes<W>(&self, mut writer: W) -> std::io::Result<usize>
    where
        W: std::io::Write,
    {
        let id_length =
            message::write_length_id_pair(&mut writer, message::REQUEST_MESSAGE_LEN, Some(message::REQUEST_MESSAGE_ID))?;

        let () = writer.write_u32::<BigEndian>(self.piece_index)?;
        let () = writer.write_u32::<BigEndian>(self.block_offset)?;
        {
            let block_length: u32 = self
                .block_length()
                .try_into()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Unsupported, e))?;
            let () = writer.write_u32::<BigEndian>(block_length)?;
        }

        Ok(id_length + 12) // + u32 * 3
    }

    /// Returns the piece index of the `RequestMessage`.
    ///
    /// # Returns
    ///
    /// The piece index.
    #[must_use]
    pub fn piece_index(&self) -> u32 {
        self.piece_index
    }

    /// Returns the block offset of the `RequestMessage`.
    ///
    /// # Returns
    ///
    /// The block offset.
    #[must_use]
    pub fn block_offset(&self) -> u32 {
        self.block_offset
    }

    /// Returns the block length of the `RequestMessage`.
    ///
    /// # Returns
    ///
    /// The block length.
    #[must_use]
    pub fn block_length(&self) -> usize {
        self.block_length
    }
}

/// Parses a byte slice into a `RequestMessage`.
///
/// # Parameters
///
/// - `bytes`: The byte slice to parse.
///
/// # Returns
///
/// An `IResult` containing the remaining byte slice and an `io::Result` with the parsed `RequestMessage`.
///
/// # Errors
///
/// This function will return an error if the byte slice cannot be parsed into a `RequestMessage`.
fn parse_request(bytes: &[u8]) -> IResult<&[u8], std::io::Result<RequestMessage>> {
    map(tuple((be_u32, be_u32, be_u32)), |(index, offset, length)| {
        Ok(RequestMessage::new(index, offset, message::u32_to_usize(length)))
    })(bytes)
}

// ----------------------------------------------------------------------------//

/// Message for sending a block to a peer.
///
/// This message is shallow, meaning it contains the initial message data,
/// but the actual block should be sent to the peer after sending this message.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct PieceMessage {
    piece_index: u32,
    block_offset: u32,
    block: Bytes,
}

impl PieceMessage {
    /// Creates a new `PieceMessage`.
    ///
    /// # Parameters
    ///
    /// - `piece_index`: The index of the piece.
    /// - `block_offset`: The offset within the piece.
    /// - `block`: The block of data.
    ///
    /// # Returns
    ///
    /// A new `PieceMessage` instance.
    pub fn new(piece_index: u32, block_offset: u32, block: Bytes) -> PieceMessage {
        // TODO: Check that users Bytes wont overflow a u32
        PieceMessage {
            piece_index,
            block_offset,
            block,
        }
    }

    /// Parses a byte slice into a `PieceMessage`.
    ///
    /// # Parameters
    ///
    /// - `bytes`: The byte slice to parse.
    /// - `len`: The length of the piece.
    ///
    /// # Returns
    ///
    /// An `io::Result` containing the parsed `PieceMessage` or an error if parsing fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the byte slice cannot be parsed into a `PieceMessage`.
    pub fn parse_bytes(bytes: &[u8], len: usize) -> std::io::Result<PieceMessage> {
        match parse_piece(bytes, len) {
            Ok((_, msg)) => msg,
            Err(_) => Err(std::io::Error::new(std::io::ErrorKind::Other, "Failed to parse PieceMessage")),
        }
    }

    /// Writes the current state of the `PieceMessage` as bytes.
    ///
    /// # Parameters
    ///
    /// - `writer`: The writer to which the bytes will be written.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to write bytes.
    ///
    /// # Panics
    ///
    /// This function will panic if the block length is too large.
    pub fn write_bytes<W>(&self, mut writer: W) -> std::io::Result<usize>
    where
        W: std::io::Write,
    {
        let block_length: u32 = self
            .block_length()
            .try_into()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Unsupported, e))?;

        let actual_length = self.block_length() + 9; // + Some(message::PIECE_MESSAGE_ID) + 2 * u32

        let length_length = message::write_length_id_pair(
            &mut writer,
            actual_length.try_into().unwrap(),
            Some(message::PIECE_MESSAGE_ID),
        )?;

        let () = writer.write_u32::<BigEndian>(self.piece_index)?;
        let () = writer.write_u32::<BigEndian>(self.block_offset)?;
        let () = writer.write_all(&self.block[..])?;

        Ok(length_length + block_length as usize + 8) // + 2 * u32
    }

    /// Returns the piece index of the `PieceMessage`.
    ///
    /// # Returns
    ///
    /// The piece index.
    #[must_use]
    pub fn piece_index(&self) -> u32 {
        self.piece_index
    }

    /// Returns the block offset of the `PieceMessage`.
    ///
    /// # Returns
    ///
    /// The block offset.
    #[must_use]
    pub fn block_offset(&self) -> u32 {
        self.block_offset
    }

    /// Returns the block length of the `PieceMessage`.
    ///
    /// # Returns
    ///
    /// The block length.
    #[must_use]
    pub fn block_length(&self) -> usize {
        self.block.len()
    }

    /// Returns the block of the `PieceMessage`.
    ///
    /// # Returns
    ///
    /// The block.
    pub fn block(&self) -> Bytes {
        self.block.clone()
    }
}

/// Parses a byte slice into a `PieceMessage`.
///
/// # Parameters
///
/// - `bytes`: The byte slice to parse.
/// - `len`: The length of the piece.
///
/// # Returns
///
/// An `IResult` containing the remaining byte slice and an `io::Result` with the parsed `PieceMessage`.
///
/// # Errors
///
/// This function will return an error if the byte slice cannot be parsed into a `PieceMessage`.
fn parse_piece(bytes: &[u8], len: usize) -> IResult<&[u8], std::io::Result<PieceMessage>> {
    map(
        tuple((be_u32, be_u32, take(len - 8))),
        |(piece_index, block_offset, block): (u32, u32, &[u8])| {
            Ok(PieceMessage::new(piece_index, block_offset, Bytes::copy_from_slice(block)))
        },
    )(bytes)
}

// ----------------------------------------------------------------------------//

/// Message for cancelling a `RequestMessage` sent to a peer.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct CancelMessage {
    piece_index: u32,
    block_offset: u32,
    block_length: usize,
}

impl CancelMessage {
    /// Creates a new `CancelMessage`.
    ///
    /// # Parameters
    ///
    /// - `piece_index`: The index of the piece.
    /// - `block_offset`: The offset within the piece.
    /// - `block_length`: The length of the block.
    ///
    /// # Returns
    ///
    /// A new `CancelMessage` instance.
    #[must_use]
    pub fn new(piece_index: u32, block_offset: u32, block_length: usize) -> CancelMessage {
        CancelMessage {
            piece_index,
            block_offset,
            block_length,
        }
    }

    /// Parses a byte slice into a `CancelMessage`.
    ///
    /// # Parameters
    ///
    /// - `bytes`: The byte slice to parse.
    ///
    /// # Returns
    ///
    /// An `io::Result` containing the parsed `CancelMessage` or an error if parsing fails.
    ///
    /// # Errors
    ///
    /// This function will return an error if the byte slice cannot be parsed into a `CancelMessage`.
    pub fn parse_bytes(bytes: &[u8]) -> std::io::Result<CancelMessage> {
        match parse_cancel(bytes) {
            Ok((_, msg)) => msg,
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to parse CancelMessage",
            )),
        }
    }

    /// Writes the current state of the `CancelMessage` as bytes.
    ///
    /// # Parameters
    ///
    /// - `writer`: The writer to which the bytes will be written.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to write bytes.
    ///
    /// # Panics
    ///
    /// This function will panic if the block length is too large.
    pub fn write_bytes<W>(&self, mut writer: W) -> std::io::Result<usize>
    where
        W: std::io::Write,
    {
        let id_length =
            message::write_length_id_pair(&mut writer, message::CANCEL_MESSAGE_LEN, Some(message::CANCEL_MESSAGE_ID))?;

        let () = writer.write_u32::<BigEndian>(self.piece_index)?;
        let () = writer.write_u32::<BigEndian>(self.block_offset)?;
        {
            let block_length: u32 = self
                .block_length
                .try_into()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Unsupported, e))?;
            let () = writer.write_u32::<BigEndian>(block_length)?;
        }

        Ok(id_length + 12) // + 3 * u32
    }

    /// Returns the piece index of the `CancelMessage`.
    ///
    /// # Returns
    ///
    /// The piece index.
    #[must_use]
    pub fn piece_index(&self) -> u32 {
        self.piece_index
    }

    /// Returns the block offset of the `CancelMessage`.
    ///
    /// # Returns
    ///
    /// The block offset.
    #[must_use]
    pub fn block_offset(&self) -> u32 {
        self.block_offset
    }

    /// Returns the block length of the `CancelMessage`.
    ///
    /// # Returns
    ///
    /// The block length.
    #[must_use]
    pub fn block_length(&self) -> usize {
        self.block_length
    }
}

/// Parses a byte slice into a `CancelMessage`.
///
/// # Parameters
///
/// - `bytes`: The byte slice to parse.
///
/// # Returns
///
/// An `IResult` containing the remaining byte slice and an `io::Result` with the parsed `CancelMessage`.
///
/// # Errors
///
/// This function will return an error if the byte slice cannot be parsed into a `CancelMessage`.
fn parse_cancel(bytes: &[u8]) -> IResult<&[u8], std::io::Result<CancelMessage>> {
    map(tuple((be_u32, be_u32, be_u32)), |(index, offset, length)| {
        Ok(CancelMessage::new(index, offset, message::u32_to_usize(length)))
    })(bytes)
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::{BitFieldMessage, HaveMessage};

    #[test]
    fn positive_bitfield_iter_empty() {
        let bitfield = BitFieldMessage::new(Bytes::new());

        assert_eq!(0, bitfield.iter().count());
    }

    #[test]
    fn positive_bitfield_iter_no_messages() {
        let bitfield = BitFieldMessage::new(Bytes::copy_from_slice(&[0x00, 0x00, 0x00]));

        assert_eq!(0, bitfield.iter().count());
    }

    #[test]
    fn positive_bitfield_iter_single_message_beginning() {
        let bitfield = BitFieldMessage::new(Bytes::copy_from_slice(&[0x80, 0x00, 0x00]));

        assert_eq!(1, bitfield.iter().count());
        assert_eq!(HaveMessage::new(0), bitfield.iter().next().unwrap());
    }

    #[test]
    fn positive_bitfield_iter_single_message_middle() {
        let bitfield = BitFieldMessage::new(Bytes::copy_from_slice(&[0x00, 0x01, 0x00]));

        assert_eq!(1, bitfield.iter().count());
        assert_eq!(HaveMessage::new(15), bitfield.iter().next().unwrap());
    }

    #[test]
    fn positive_bitfield_iter_single_message_ending() {
        let bitfield = BitFieldMessage::new(Bytes::copy_from_slice(&[0x00, 0x00, 0x01]));

        assert_eq!(1, bitfield.iter().count());
        assert_eq!(HaveMessage::new(23), bitfield.iter().next().unwrap());
    }

    #[test]
    fn positive_bitfield_iter_multiple_messages() {
        let bitfield = BitFieldMessage::new(Bytes::copy_from_slice(&[0xAF, 0x00, 0xC1]));
        let messages: Vec<HaveMessage> = bitfield.iter().collect();

        assert_eq!(9, messages.len());
        assert_eq!(
            vec![
                HaveMessage::new(0),
                HaveMessage::new(2),
                HaveMessage::new(4),
                HaveMessage::new(5),
                HaveMessage::new(6),
                HaveMessage::new(7),
                HaveMessage::new(16),
                HaveMessage::new(17),
                HaveMessage::new(23)
            ],
            messages
        );
    }
}
