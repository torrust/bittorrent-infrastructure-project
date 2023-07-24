use std::io;
use std::io::Write;

use bencode::{BConvert, BDecodeOpt, BencodeRef};
use bytes::Bytes;

use crate::message::bencode_util;

const REQUEST_MESSAGE_TYPE_ID: u8 = 0;
const DATA_MESSAGE_TYPE_ID: u8 = 1;
const REJECT_MESSAGE_TYPE_ID: u8 = 2;

const ROOT_ERROR_KEY: &str = "PeerExtensionProtocolMessage";

/// Enumeration of messages for `PeerExtensionProtocolMessage::UtMetadata`.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum UtMetadataMessage {
    Request(UtMetadataRequestMessage),
    Data(UtMetadataDataMessage),
    Reject(UtMetadataRejectMessage),
}

impl UtMetadataMessage {
    pub fn parse_bytes(mut bytes: Bytes) -> io::Result<UtMetadataMessage> {
        // Our bencode is pretty flat, and we dont want to enforce a full decode, as data
        // messages have the raw data appended outside of the bencode structure...
        let decode_opts = BDecodeOpt::new(2, false, false);

        match BencodeRef::decode(bytes.clone().as_ref(), decode_opts) {
            Ok(bencode) => {
                let bencode_dict = (bencode_util::CONVERT.convert_dict(&bencode, ROOT_ERROR_KEY))?;
                let msg_type = (bencode_util::parse_message_type(bencode_dict))?;
                let piece = (bencode_util::parse_piece_index(bencode_dict))?;

                let bencode_bytes = bytes.split_to(bencode.buffer().len());
                let extra_bytes = bytes;

                match msg_type {
                    REQUEST_MESSAGE_TYPE_ID => Ok(UtMetadataMessage::Request(UtMetadataRequestMessage::with_bytes(
                        piece,
                        bencode_bytes,
                    ))),
                    REJECT_MESSAGE_TYPE_ID => Ok(UtMetadataMessage::Reject(UtMetadataRejectMessage::with_bytes(
                        piece,
                        bencode_bytes,
                    ))),
                    DATA_MESSAGE_TYPE_ID => {
                        let total_size = (bencode_util::parse_total_size(bencode_dict))?;

                        Ok(UtMetadataMessage::Data(UtMetadataDataMessage::with_bytes(
                            piece,
                            total_size,
                            extra_bytes,
                            bencode_bytes,
                        )))
                    }
                    other => Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed To Recognize Message Type For UtMetadataMessage: {msg_type}"),
                    )),
                }
            }
            Err(err) => Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Failed To Parse UtMetadataMessage As Bencode: {err}"),
            )),
        }
    }

    pub fn write_bytes<W>(&self, writer: W) -> io::Result<()>
    where
        W: Write,
    {
        match self {
            UtMetadataMessage::Request(request) => request.write_bytes(writer),
            UtMetadataMessage::Data(data) => data.write_bytes(writer),
            UtMetadataMessage::Reject(reject) => reject.write_bytes(writer),
        }
    }

    pub fn message_size(&self) -> usize {
        match self {
            UtMetadataMessage::Request(request) => request.message_size(),
            UtMetadataMessage::Data(data) => data.message_size(),
            UtMetadataMessage::Reject(reject) => reject.message_size(),
        }
    }
}

// ----------------------------------------------------------------------------//

/// Message for requesting a piece of metadata from a peer.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct UtMetadataRequestMessage {
    piece: i64,
    bencode_size: usize,
}

impl UtMetadataRequestMessage {
    #[must_use]
    pub fn new(piece: i64) -> UtMetadataRequestMessage {
        let encoded_bytes_size = (ben_map! {
            bencode_util::MESSAGE_TYPE_KEY => ben_int!(i64::from(REQUEST_MESSAGE_TYPE_ID)),
            bencode_util::PIECE_INDEX_KEY  => ben_int!(piece)
        })
        .encode()
        .len();

        UtMetadataRequestMessage {
            piece,
            bencode_size: encoded_bytes_size,
        }
    }

    pub fn with_bytes(piece: i64, bytes: Bytes) -> UtMetadataRequestMessage {
        UtMetadataRequestMessage {
            piece,
            bencode_size: bytes.len(),
        }
    }

    pub fn write_bytes<W>(&self, mut writer: W) -> io::Result<()>
    where
        W: Write,
    {
        let encoded_bytes = (ben_map! {
            bencode_util::MESSAGE_TYPE_KEY => ben_int!(i64::from(REQUEST_MESSAGE_TYPE_ID)),
            bencode_util::PIECE_INDEX_KEY  => ben_int!(self.piece)
        })
        .encode();

        writer.write_all(encoded_bytes.as_ref())
    }

    #[must_use]
    pub fn message_size(&self) -> usize {
        self.bencode_size
    }

    #[must_use]
    pub fn piece(&self) -> i64 {
        self.piece
    }
}

/// Message for sending a piece of metadata from a peer.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct UtMetadataDataMessage {
    piece: i64,
    total_size: i64,
    data: Bytes,
    bencode_size: usize,
}

impl UtMetadataDataMessage {
    pub fn new(piece: i64, total_size: i64, data: Bytes) -> UtMetadataDataMessage {
        let encoded_bytes_len = (ben_map! {
            bencode_util::MESSAGE_TYPE_KEY => ben_int!(i64::from(DATA_MESSAGE_TYPE_ID)),
            bencode_util::PIECE_INDEX_KEY  => ben_int!(piece),
            bencode_util::TOTAL_SIZE_KEY   => ben_int!(total_size)
        })
        .encode()
        .len();

        UtMetadataDataMessage {
            piece,
            total_size,
            data,
            bencode_size: encoded_bytes_len,
        }
    }

    pub fn with_bytes(piece: i64, total_size: i64, data: Bytes, bytes: Bytes) -> UtMetadataDataMessage {
        UtMetadataDataMessage {
            piece,
            total_size,
            data,
            bencode_size: bytes.len(),
        }
    }

    pub fn write_bytes<W>(&self, mut writer: W) -> io::Result<()>
    where
        W: Write,
    {
        let encoded_bytes = (ben_map! {
            bencode_util::MESSAGE_TYPE_KEY => ben_int!(i64::from(DATA_MESSAGE_TYPE_ID)),
            bencode_util::PIECE_INDEX_KEY  => ben_int!(self.piece),
            bencode_util::TOTAL_SIZE_KEY   => ben_int!(self.total_size)
        })
        .encode();

        (writer.write_all(encoded_bytes.as_ref()))?;

        writer.write_all(self.data.as_ref())
    }

    pub fn message_size(&self) -> usize {
        self.bencode_size + self.data.len()
    }

    pub fn piece(&self) -> i64 {
        self.piece
    }

    pub fn total_size(&self) -> i64 {
        self.total_size
    }

    pub fn data(&self) -> &Bytes {
        &self.data
    }
}

/// Message for rejecting a request for metadata from a peer.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct UtMetadataRejectMessage {
    piece: i64,
    bencode_size: usize,
}

impl UtMetadataRejectMessage {
    #[must_use]
    pub fn new(piece: i64) -> UtMetadataRejectMessage {
        let encoded_bytes_size = (ben_map! {
            bencode_util::MESSAGE_TYPE_KEY => ben_int!(i64::from(REJECT_MESSAGE_TYPE_ID)),
            bencode_util::PIECE_INDEX_KEY  => ben_int!(piece)
        })
        .encode()
        .len();

        UtMetadataRejectMessage {
            piece,
            bencode_size: encoded_bytes_size,
        }
    }

    pub fn with_bytes(piece: i64, bytes: Bytes) -> UtMetadataRejectMessage {
        UtMetadataRejectMessage {
            piece,
            bencode_size: bytes.len(),
        }
    }

    pub fn write_bytes<W>(&self, mut writer: W) -> io::Result<()>
    where
        W: Write,
    {
        let encoded_bytes = (ben_map! {
            bencode_util::MESSAGE_TYPE_KEY => ben_int!(i64::from(REJECT_MESSAGE_TYPE_ID)),
            bencode_util::PIECE_INDEX_KEY  => ben_int!(self.piece)
        })
        .encode();

        writer.write_all(encoded_bytes.as_ref())
    }

    #[must_use]
    pub fn message_size(&self) -> usize {
        self.bencode_size
    }

    #[must_use]
    pub fn piece(&self) -> i64 {
        self.piece
    }
}
