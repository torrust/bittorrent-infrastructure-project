#![allow(unused)]

//! Serializable and deserializable protocol messages.

// Nom has lots of unused warnings atm, keep this here for now.

use std::io::Write as _;

use byteorder::{BigEndian, WriteBytesExt};
use bytes::Bytes;
use nom::branch::alt;
use nom::bytes::complete::take;
use nom::combinator::{all_consuming, map, map_res, opt, value};
use nom::number::complete::{be_u32, be_u8};
use nom::sequence::{preceded, tuple};
use nom::IResult;
use thiserror::Error;

use crate::protocol::PeerProtocol;

// TODO: Propagate failures to cast values to/from usize

const KEEP_ALIVE_MESSAGE_LEN: u32 = 0;
const CHOKE_MESSAGE_LEN: u32 = 1;
const UNCHOKE_MESSAGE_LEN: u32 = 1;
const INTERESTED_MESSAGE_LEN: u32 = 1;
const UNINTERESTED_MESSAGE_LEN: u32 = 1;
const HAVE_MESSAGE_LEN: u32 = 5;
const BASE_BITFIELD_MESSAGE_LEN: u32 = 1;
const REQUEST_MESSAGE_LEN: u32 = 13;
const BASE_PIECE_MESSAGE_LEN: u32 = 9;
const CANCEL_MESSAGE_LEN: u32 = 13;

const CHOKE_MESSAGE_ID: u8 = 0;
const UNCHOKE_MESSAGE_ID: u8 = 1;
const INTERESTED_MESSAGE_ID: u8 = 2;
const UNINTERESTED_MESSAGE_ID: u8 = 3;
const HAVE_MESSAGE_ID: u8 = 4;
const BITFIELD_MESSAGE_ID: u8 = 5;
const REQUEST_MESSAGE_ID: u8 = 6;
const PIECE_MESSAGE_ID: u8 = 7;
const CANCEL_MESSAGE_ID: u8 = 8;

const MESSAGE_LENGTH_LEN_BYTES: usize = 4;
const MESSAGE_ID_LEN_BYTES: usize = 1;
const HEADER_LEN: usize = MESSAGE_LENGTH_LEN_BYTES + MESSAGE_ID_LEN_BYTES;

mod bencode_util;
mod bits_ext;
mod null;
mod prot_ext;
mod standard;

#[allow(clippy::module_name_repetitions)]
pub use crate::message::bits_ext::{BitsExtensionMessage, ExtendedMessage, ExtendedMessageBuilder, ExtendedType, PortMessage};
#[allow(clippy::module_name_repetitions)]
pub use crate::message::null::NullProtocolMessage;
#[allow(clippy::module_name_repetitions)]
pub use crate::message::prot_ext::{
    PeerExtensionProtocolMessage, PeerExtensionProtocolMessageError, UtMetadataDataMessage, UtMetadataMessage,
    UtMetadataRejectMessage, UtMetadataRequestMessage,
};
#[allow(clippy::module_name_repetitions)]
pub use crate::message::standard::{BitFieldIter, BitFieldMessage, CancelMessage, HaveMessage, PieceMessage, RequestMessage};
use crate::ManagedMessage;

#[derive(Error, Debug, Clone)]
pub enum PeerWireProtocolMessageError {}

impl From<PeerWireProtocolMessageError> for std::io::Error {
    fn from(err: PeerWireProtocolMessageError) -> Self {
        std::io::Error::new(std::io::ErrorKind::Other, err)
    }
}

/// Enumeration of messages for `PeerWireProtocol`.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone)]
pub enum PeerWireProtocolMessage<P>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    /// Message to keep the connection alive.
    KeepAlive,
    /// Message to tell a peer we will not be responding to their requests.
    ///
    /// Peers may wish to send *Interested and/or `KeepAlive` messages.
    Choke,
    /// Message to tell a peer we will now be responding to their requests.
    UnChoke,
    /// Message to tell a peer we are interested in downloading pieces from them.
    Interested,
    /// Message to tell a peer we are not interested in downloading pieces from them.
    UnInterested,
    /// Message to tell a peer we have some (validated) piece.
    Have(HaveMessage),
    /// Message to effectively send multiple `HaveMessages` in a single message.
    ///
    /// This message is only valid when the connection is initiated with the peer.
    BitField(BitFieldMessage),
    /// Message to request a block from a peer.
    Request(RequestMessage),
    /// Message from a peer containing a block.
    Piece(PieceMessage),
    /// Message to cancel a block request from a peer.
    Cancel(CancelMessage),
    /// Extension messages which are activated via the `ExtensionBits` as part of the handshake.
    BitsExtension(BitsExtensionMessage),
    /// Extension messages which are activated via the Extension Protocol.
    ///
    /// In reality, this can be any type that implements `ProtocolMessage` if, for example,
    /// you are running a private swarm where you know all nodes support a given message(s).
    ProtExtension(Result<P::ProtocolMessage, P::ProtocolMessageError>),
}

impl<P> ManagedMessage for PeerWireProtocolMessage<P>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    fn keep_alive() -> PeerWireProtocolMessage<P> {
        PeerWireProtocolMessage::KeepAlive
    }

    fn is_keep_alive(&self) -> bool {
        matches!(self, &PeerWireProtocolMessage::KeepAlive)
    }
}

impl<P> PeerWireProtocolMessage<P>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    /// Bytes Needed to encode Byte Slice
    ///
    /// # Errors
    ///
    /// This function will not return an error.
    pub fn bytes_needed(bytes: &[u8]) -> std::io::Result<Option<usize>> {
        match be_u32::<_, nom::error::Error<&[u8]>>(bytes) {
            Ok((_, length)) => Ok(Some(MESSAGE_LENGTH_LEN_BYTES + u32_to_usize(length))),
            _ => Ok(None),
        }
    }

    /// Parse Bytes into a [`PeerWireProtocolMessage`]
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to parse bytes for supplied protocol.
    pub fn parse_bytes(bytes: &[u8], ext_protocol: &mut P) -> std::io::Result<PeerWireProtocolMessage<P>> {
        match parse_message(bytes, ext_protocol) {
            Ok((_, result)) => result,
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed To Parse PeerWireProtocolMessage",
            )),
        }
    }

    /// Write out current states as bytes.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to write bytes.
    pub fn write_bytes<W>(&self, writer: W, ext_protocol: &mut P) -> std::io::Result<usize>
    where
        W: std::io::Write,
    {
        match self {
            &PeerWireProtocolMessage::KeepAlive => write_length_id_pair(writer, KEEP_ALIVE_MESSAGE_LEN, None),
            &PeerWireProtocolMessage::Choke => write_length_id_pair(writer, CHOKE_MESSAGE_LEN, Some(CHOKE_MESSAGE_ID)),
            &PeerWireProtocolMessage::UnChoke => write_length_id_pair(writer, UNCHOKE_MESSAGE_LEN, Some(UNCHOKE_MESSAGE_ID)),
            &PeerWireProtocolMessage::Interested => {
                write_length_id_pair(writer, INTERESTED_MESSAGE_LEN, Some(INTERESTED_MESSAGE_ID))
            }
            &PeerWireProtocolMessage::UnInterested => {
                write_length_id_pair(writer, UNINTERESTED_MESSAGE_LEN, Some(UNINTERESTED_MESSAGE_ID))
            }
            PeerWireProtocolMessage::Have(msg) => msg.write_bytes(writer),
            PeerWireProtocolMessage::BitField(msg) => msg.write_bytes(writer),
            PeerWireProtocolMessage::Request(msg) => msg.write_bytes(writer),
            PeerWireProtocolMessage::Piece(msg) => msg.write_bytes(writer),
            PeerWireProtocolMessage::Cancel(msg) => msg.write_bytes(writer),
            PeerWireProtocolMessage::BitsExtension(ext) => ext.write_bytes(writer),
            PeerWireProtocolMessage::ProtExtension(ext) => ext_protocol.write_bytes(ext, writer),
        }
    }

    /// Retrieve how many bytes the message will occupy on the wire.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to calculate the message length.
    pub fn message_size(&self, ext_protocol: &mut P) -> std::io::Result<usize> {
        let message_specific_len = match self {
            &PeerWireProtocolMessage::KeepAlive => KEEP_ALIVE_MESSAGE_LEN as usize,
            &PeerWireProtocolMessage::Choke => CHOKE_MESSAGE_LEN as usize,
            &PeerWireProtocolMessage::UnChoke => UNCHOKE_MESSAGE_LEN as usize,
            &PeerWireProtocolMessage::Interested => INTERESTED_MESSAGE_LEN as usize,
            &PeerWireProtocolMessage::UnInterested => UNINTERESTED_MESSAGE_LEN as usize,
            &PeerWireProtocolMessage::Have(_) => HAVE_MESSAGE_LEN as usize,
            PeerWireProtocolMessage::BitField(msg) => BASE_BITFIELD_MESSAGE_LEN as usize + msg.bitfield().len(),
            &PeerWireProtocolMessage::Request(_) => REQUEST_MESSAGE_LEN as usize,
            PeerWireProtocolMessage::Piece(msg) => BASE_PIECE_MESSAGE_LEN as usize + msg.block().len(),
            &PeerWireProtocolMessage::Cancel(_) => CANCEL_MESSAGE_LEN as usize,
            PeerWireProtocolMessage::BitsExtension(ext) => ext.message_size(),
            PeerWireProtocolMessage::ProtExtension(ext) => ext_protocol.message_size(ext)?,
        };

        Ok(MESSAGE_LENGTH_LEN_BYTES + message_specific_len)
    }
}

/// Write a length and optional id out to the given writer.
fn write_length_id_pair<W>(mut writer: W, length: u32, opt_id: Option<u8>) -> std::io::Result<usize>
where
    W: std::io::Write,
{
    writer.write_u32::<BigEndian>(length)?;

    if let Some(id) = opt_id {
        let () = writer.write_u8(id)?;
        Ok(5)
    } else {
        Ok(4)
    }
}

/// Parse the length portion of a message.
///
/// Panics if parsing failed for any reason.
fn parse_message_length(bytes: &[u8]) -> usize {
    if let Ok((_, len)) = be_u32::<_, nom::error::Error<&[u8]>>(bytes) {
        u32_to_usize(len)
    } else {
        panic!("bip_peer: Message Length Was Less Than 4 Bytes")
    }
}

/// Panics if the conversion from a u32 to usize is not valid.
fn u32_to_usize(value: u32) -> usize {
    value.try_into().expect("it should be able to convert from u32 to usize")
}

// Since these messages may come over a stream oriented protocol, if a message is incomplete
// the number of bytes needed will be returned. However, that number of bytes is on a per parser
// basis. If possible, we should return the number of bytes needed for the rest of the WHOLE message.
// This allows clients to only re invoke the parser when it knows it has enough of the data.

fn parse_keep_alive<P>(input: &[u8]) -> IResult<&[u8], std::io::Result<PeerWireProtocolMessage<P>>>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    map(
        tuple((
            be_u32::<_, nom::error::Error<&[u8]>>,
            opt(be_u8::<_, nom::error::Error<&[u8]>>),
        )),
        |_| Ok(PeerWireProtocolMessage::KeepAlive),
    )(input)
}

fn parse_choke<P>(input: &[u8]) -> IResult<&[u8], std::io::Result<PeerWireProtocolMessage<P>>>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    map(
        tuple((
            value(CHOKE_MESSAGE_LEN, be_u32::<_, nom::error::Error<&[u8]>>),
            value(Some(CHOKE_MESSAGE_ID), be_u8::<_, nom::error::Error<&[u8]>>),
        )),
        |_| Ok(PeerWireProtocolMessage::Choke),
    )(input)
}

fn parse_unchoke<P>(input: &[u8]) -> IResult<&[u8], std::io::Result<PeerWireProtocolMessage<P>>>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    map(
        tuple((
            value(UNCHOKE_MESSAGE_LEN, be_u32::<_, nom::error::Error<&[u8]>>),
            value(Some(UNCHOKE_MESSAGE_ID), be_u8::<_, nom::error::Error<&[u8]>>),
        )),
        |_| Ok(PeerWireProtocolMessage::UnChoke),
    )(input)
}

fn parse_interested<P>(input: &[u8]) -> IResult<&[u8], std::io::Result<PeerWireProtocolMessage<P>>>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    map(
        tuple((
            value(INTERESTED_MESSAGE_LEN, be_u32::<_, nom::error::Error<&[u8]>>),
            value(Some(INTERESTED_MESSAGE_ID), be_u8::<_, nom::error::Error<&[u8]>>),
        )),
        |_| Ok(PeerWireProtocolMessage::Interested),
    )(input)
}

fn parse_uninterested<P>(input: &[u8]) -> IResult<&[u8], std::io::Result<PeerWireProtocolMessage<P>>>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    map(
        tuple((
            value(UNINTERESTED_MESSAGE_LEN, be_u32::<_, nom::error::Error<&[u8]>>),
            value(Some(UNINTERESTED_MESSAGE_ID), be_u8::<_, nom::error::Error<&[u8]>>),
        )),
        |_| Ok(PeerWireProtocolMessage::UnInterested),
    )(input)
}

fn parse_have<P>(input: &[u8]) -> IResult<&[u8], std::io::Result<PeerWireProtocolMessage<P>>>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    map(
        preceded(
            tuple((
                value(HAVE_MESSAGE_LEN, be_u32::<_, nom::error::Error<&[u8]>>),
                value(Some(HAVE_MESSAGE_ID), be_u8::<_, nom::error::Error<&[u8]>>),
            )),
            take(4_usize),
        ),
        |have| HaveMessage::parse_bytes(have).map(PeerWireProtocolMessage::Have),
    )(input)
}

fn parse_bitfield<P>(input: &[u8]) -> IResult<&[u8], std::io::Result<PeerWireProtocolMessage<P>>>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    map(
        preceded(
            tuple((
                value(BASE_BITFIELD_MESSAGE_LEN, be_u32::<_, nom::error::Error<&[u8]>>),
                value(Some(BITFIELD_MESSAGE_ID), be_u8::<_, nom::error::Error<&[u8]>>),
            )),
            take(4_usize),
        ),
        |bitfield| BitFieldMessage::parse_bytes(bitfield).map(PeerWireProtocolMessage::BitField),
    )(input)
}

fn parse_request<P>(input: &[u8]) -> IResult<&[u8], std::io::Result<PeerWireProtocolMessage<P>>>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    map(
        preceded(
            tuple((
                value(REQUEST_MESSAGE_LEN, be_u32::<_, nom::error::Error<&[u8]>>),
                value(Some(REQUEST_MESSAGE_ID), be_u8::<_, nom::error::Error<&[u8]>>),
            )),
            take(4_usize),
        ),
        |request| RequestMessage::parse_bytes(request).map(PeerWireProtocolMessage::Request),
    )(input)
}

fn parse_piece<P>(input: &[u8]) -> IResult<&[u8], std::io::Result<PeerWireProtocolMessage<P>>>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    map(
        preceded(
            tuple((
                value(BASE_PIECE_MESSAGE_LEN, be_u32::<_, nom::error::Error<&[u8]>>),
                value(Some(PIECE_MESSAGE_ID), be_u8::<_, nom::error::Error<&[u8]>>),
            )),
            take(4_usize),
        ),
        |piece| {
            let len = parse_message_length(piece);
            PieceMessage::parse_bytes(piece, len).map(PeerWireProtocolMessage::Piece)
        },
    )(input)
}

fn parse_cancel<P>(input: &[u8]) -> IResult<&[u8], std::io::Result<PeerWireProtocolMessage<P>>>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    map(
        preceded(
            tuple((
                value(CANCEL_MESSAGE_LEN, be_u32::<_, nom::error::Error<&[u8]>>),
                value(Some(CANCEL_MESSAGE_ID), be_u8::<_, nom::error::Error<&[u8]>>),
            )),
            take(4_usize),
        ),
        |cancel| CancelMessage::parse_bytes(cancel).map(PeerWireProtocolMessage::Cancel),
    )(input)
}

fn parse_bits_extension<P>(input: &[u8]) -> IResult<&[u8], std::io::Result<PeerWireProtocolMessage<P>>>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    map(
        |input| BitsExtensionMessage::parse_bytes(input),
        |res_bits_ext| res_bits_ext.map(|bits_ext| PeerWireProtocolMessage::BitsExtension(bits_ext)),
    )(input)
}

fn parse_prot_extension<'a, P>(
    input: &'a [u8],
    ext_protocol: &mut P,
) -> IResult<&'a [u8], std::io::Result<PeerWireProtocolMessage<P>>>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    map(
        |input| match ext_protocol.parse_bytes(input) {
            Ok(msg) => Ok((input, Ok(PeerWireProtocolMessage::ProtExtension(msg)))),
            Err(_) => Err(nom::Err::Error(nom::error::Error {
                input,
                code: nom::error::ErrorKind::Fail,
            })),
        },
        |result| result,
    )(input)
}

fn parse_message<'a, P>(bytes: &'a [u8], ext_protocol: &mut P) -> IResult<&'a [u8], std::io::Result<PeerWireProtocolMessage<P>>>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    alt((
        parse_keep_alive,
        parse_choke,
        parse_unchoke,
        parse_interested,
        parse_uninterested,
        parse_have,
        parse_bitfield,
        parse_request,
        parse_piece,
        parse_cancel,
        parse_bits_extension,
        |input| parse_prot_extension(input, ext_protocol),
    ))(bytes)
}
