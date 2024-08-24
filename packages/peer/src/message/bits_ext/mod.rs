use std::collections::HashMap;
use std::io::Write as _;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use bencode::{BConvert, BDecodeOpt, BMutAccess, BencodeMut, BencodeRef};
use byteorder::{BigEndian, WriteBytesExt};
use bytes::Bytes;
use nom::branch::alt;
use nom::bytes::complete::{take, take_while};
use nom::combinator::map;
use nom::error::Error;
use nom::number::complete::{be_u16, be_u32, be_u8};
use nom::sequence::tuple;
use nom::IResult;
use util::convert;

use crate::message;
use crate::message::bencode_util;

const PORT_MESSAGE_LEN: u32 = 3;
const BASE_EXTENDED_MESSAGE_LEN: u32 = 6;

const PORT_MESSAGE_ID: u8 = 9;
pub const EXTENDED_MESSAGE_ID: u8 = 20;

const EXTENDED_MESSAGE_HANDSHAKE_ID: u8 = 0;

mod handshake;
mod port;

pub use self::handshake::{ExtendedMessage, ExtendedMessageBuilder, ExtendedType};
pub use self::port::PortMessage;

/// Enumeration of messages for `PeerWireProtocolMessage`, activated via `Extensions` bits.
///
/// Sent after the handshake if the corresponding extension bit is set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BitsExtensionMessage {
    /// Message for determining the port a peer's DHT is listening on.
    Port(PortMessage),
    /// Message for sending a peer the map of extensions we support.
    Extended(ExtendedMessage),
}

impl BitsExtensionMessage {
    /// Parses a byte slice into a `BitsExtensionMessage`.
    ///
    /// # Parameters
    ///
    /// - `input`: The byte slice to parse.
    ///
    /// # Returns
    ///
    /// An `IResult` containing the remaining byte slice and an `io::Result` with the parsed `BitsExtensionMessage`.
    ///
    /// # Errors
    ///
    /// This function will return an error if the byte slice cannot be parsed into a `BitsExtensionMessage`.
    pub fn parse_bytes<'a>(input: &'a [u8]) -> IResult<&'a [u8], std::io::Result<BitsExtensionMessage>> {
        let port_fn = |input: &'a [u8]| -> IResult<&'a [u8], std::io::Result<BitsExtensionMessage>> {
            let (_, (message_len, message_id)) = tuple((be_u32, be_u8))(input)?;

            if (message_len, message_id) == (PORT_MESSAGE_LEN, PORT_MESSAGE_ID) {
                let (_, res_port) = PortMessage::parse_bytes(&input[message::HEADER_LEN..])?;
                Ok((input, Ok(BitsExtensionMessage::Port(res_port))))
            } else {
                Err(nom::Err::Error(nom::error::Error {
                    input,
                    code: nom::error::ErrorKind::Switch,
                }))
            }
        };

        let ext_fn = |input: &'a [u8]| -> IResult<&'a [u8], std::io::Result<BitsExtensionMessage>> {
            let (_, (message_len, extended_message_id, extended_message_handshake_id)) = tuple((be_u32, be_u8, be_u8))(input)?;

            if (message_len, extended_message_id, extended_message_handshake_id)
                == (message_len, EXTENDED_MESSAGE_ID, EXTENDED_MESSAGE_HANDSHAKE_ID)
            {
                let (_, res_extended) = ExtendedMessage::parse_bytes(&input[message::HEADER_LEN + 1..], message_len - 2)?;
                Ok((input, res_extended.map(BitsExtensionMessage::Extended)))
            } else {
                Err(nom::Err::Error(nom::error::Error {
                    input,
                    code: nom::error::ErrorKind::Switch,
                }))
            }
        };

        alt((port_fn, ext_fn))(input)
    }

    /// Writes the current state of the `BitsExtensionMessage` as bytes.
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
        match self {
            BitsExtensionMessage::Port(msg) => msg.write_bytes(writer),
            BitsExtensionMessage::Extended(msg) => msg.write_bytes(writer),
        }
    }

    /// Returns the size of the `BitsExtensionMessage`.
    ///
    /// # Returns
    ///
    /// The size of the message in bytes.
    pub fn message_size(&self) -> usize {
        match self {
            BitsExtensionMessage::Port(_) => PORT_MESSAGE_LEN as usize,
            BitsExtensionMessage::Extended(msg) => BASE_EXTENDED_MESSAGE_LEN as usize + msg.bencode_size(),
        }
    }
}
