use std::io::Write as _;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use bencode::{BConvert, BDecodeOpt, BencodeRef};
use byteorder::{BigEndian, WriteBytesExt};
use bytes::Bytes;
use nom::branch::alt;
use nom::bytes::complete::{take, take_until};
use nom::combinator::{map, value};
use nom::error::{ErrorKind, ParseError};
use nom::multi::length_data;
use nom::number::complete::{be_u32, be_u8};
use nom::sequence::{pair, tuple};
use nom::IResult;
use thiserror::Error;
use ut_metadata::UtMetadataMessageError;

use crate::message::{self, bencode_util, bits_ext, ExtendedMessage, ExtendedType, PeerWireProtocolMessage};
use crate::protocol::PeerProtocol;

const EXTENSION_HEADER_LEN: usize = message::HEADER_LEN + 1;

mod ut_metadata;

pub use self::ut_metadata::{UtMetadataDataMessage, UtMetadataMessage, UtMetadataRejectMessage, UtMetadataRequestMessage};

#[derive(Debug, Clone)]

pub struct ByteVecDisplay(Vec<u8>);

impl std::fmt::Display for ByteVecDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ByteVecDisplay").field(&self.0).finish()
    }
}

impl Deref for ByteVecDisplay {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Error, Debug, Clone)]
pub enum PeerExtensionProtocolMessageError {
    #[error("Error from UtMetadata")]
    UtMetadataError(UtMetadataMessageError),

    #[error("Error from UtMetadata")]
    UnknownId(),

    #[error("Failed to Parse Extension Id: {0}")]
    UnknownExtensionId(u8),

    #[error("Failed to Parse Extension: {0}")]
    ParseExtensionError(Arc<nom::Err<nom::error::Error<ByteVecDisplay>>>),
}

impl From<UtMetadataMessageError> for PeerExtensionProtocolMessageError {
    fn from(value: UtMetadataMessageError) -> Self {
        Self::UtMetadataError(value)
    }
}

/// Enumeration of `BEP 10` extension protocol compatible messages.
#[derive(Debug)]
pub enum PeerExtensionProtocolMessage<P>
where
    P: PeerProtocol,
{
    UtMetadata(UtMetadataMessage),
    //UtPex(UtPexMessage),
    Custom(Result<P::ProtocolMessage, P::ProtocolMessageError>),
}

impl<P> PeerExtensionProtocolMessage<P>
where
    P: PeerProtocol + Clone + std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessage: std::fmt::Debug,
    <P as PeerProtocol>::ProtocolMessageError: std::fmt::Debug,
{
    /// Returns the number of bytes needed encode a given slice.
    ///
    /// # Errors
    ///
    /// This function should not return an error.
    pub fn bytes_needed(bytes: &[u8]) -> std::io::Result<Option<usize>> {
        // Follows same length prefix logic as our normal wire protocol...
        PeerWireProtocolMessage::<P>::bytes_needed(bytes)
    }

    /// Parse Bytes to create a [`PeerExtensionProtocolMessage`]
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to parse.
    pub fn parse_bytes(
        bytes: &[u8],
        extended: &ExtendedMessage,
        custom_prot: &mut P,
    ) -> std::io::Result<Result<PeerExtensionProtocolMessage<P>, PeerExtensionProtocolMessageError>> {
        // pass through an inner `std::io::Error`, and wrap any nom-error.
        let res = match parse_extensions(bytes, extended, custom_prot) {
            Ok((_, result)) => result?,
            Err(err) => Err(PeerExtensionProtocolMessageError::ParseExtensionError(Arc::new(
                err.to_owned().map_input(ByteVecDisplay),
            ))),
        };

        Ok(res)
    }

    /// Write Bytes from the current state.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to write the bytes.
    ///
    /// # Panics
    ///
    /// This function will panic if the message is too long.
    pub fn write_bytes<W>(&self, mut writer: W, extended: &ExtendedMessage, custom_prot: &mut P) -> std::io::Result<usize>
    where
        W: std::io::Write,
    {
        match self {
            PeerExtensionProtocolMessage::UtMetadata(msg) => {
                let Some(ext_id) = extended.query_id(&ExtendedType::UtMetadata) else {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Can't Send UtMetadataMessage As We Have No Id Mapping",
                    ));
                };

                let total_len = (2 + msg.message_size());

                let id_length = message::write_length_id_pair(
                    &mut writer,
                    total_len.try_into().unwrap(),
                    Some(bits_ext::EXTENDED_MESSAGE_ID),
                )?;
                writer.write_u8(ext_id)?;

                let () = msg.write_bytes(writer)?;

                Ok(id_length + total_len)
            }
            PeerExtensionProtocolMessage::Custom(msg) => custom_prot.write_bytes(msg, writer),
        }
    }

    /// Retrieve how many bytes the message will occupy on the wire.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to calculate the message length.
    pub fn message_size(&self, custom_prot: &mut P) -> std::io::Result<usize> {
        match self {
            PeerExtensionProtocolMessage::UtMetadata(msg) => Ok(msg.message_size()),
            PeerExtensionProtocolMessage::Custom(msg) => custom_prot.message_size(msg),
        }
    }
}

fn parse_extensions<'a, P>(
    bytes: &'a [u8],
    extended: &ExtendedMessage,
    custom_prot: &mut P,
) -> IResult<&'a [u8], std::io::Result<Result<PeerExtensionProtocolMessage<P>, PeerExtensionProtocolMessageError>>>
where
    P: PeerProtocol,
{
    let ut_metadata_fn = |input: &'a [u8]| -> IResult<
        &'a [u8],
        std::io::Result<Result<PeerExtensionProtocolMessage<P>, PeerExtensionProtocolMessageError>>,
    > {
        let (_, (message_len, extended_message_id, message_id)) = tuple((be_u32, be_u8, be_u8))(input)?;

        if extended_message_id == bits_ext::EXTENDED_MESSAGE_ID {
            let from = EXTENSION_HEADER_LEN;
            let to = EXTENSION_HEADER_LEN + message_len as usize - 2;

            let bytes = Bytes::copy_from_slice(&input[from..to]);

            Ok((input, parse_extensions_with_id(bytes, extended, message_id)))
        } else {
            Ok((
                input,
                Ok(Err(PeerExtensionProtocolMessageError::UnknownExtensionId(
                    extended_message_id,
                ))),
            ))
        }
    };

    let custom_fn = |input: &'a [u8]| -> IResult<
        &'a [u8],
        std::io::Result<Result<PeerExtensionProtocolMessage<P>, PeerExtensionProtocolMessageError>>,
    > {
        Ok((
            input,
            custom_prot
                .parse_bytes(input)
                .map(|item| Ok(PeerExtensionProtocolMessage::Custom(item))),
        ))
    };

    // Attempt to parse a built in message type, otherwise, see if it is an extension type.
    alt((ut_metadata_fn, custom_fn))(bytes)
}

fn parse_extensions_with_id<P>(
    bytes: Bytes,
    extended: &ExtendedMessage,
    id: u8,
) -> std::io::Result<Result<PeerExtensionProtocolMessage<P>, PeerExtensionProtocolMessageError>>
where
    P: PeerProtocol,
{
    let Some(lt_metadata_id) = extended.query_id(&ExtendedType::UtMetadata) else {
        return Ok(Err(PeerExtensionProtocolMessageError::UnknownId()));
    };
    //let ut_pex_id = extended.query_id(&ExtendedType::UtPex);

    let item = UtMetadataMessage::parse_bytes(bytes)?;

    let item = match item {
        Ok(message) => Ok(PeerExtensionProtocolMessage::UtMetadata(message)),
        Err(err) => Err(PeerExtensionProtocolMessageError::UtMetadataError(err)),
    };

    Ok(item)
}
