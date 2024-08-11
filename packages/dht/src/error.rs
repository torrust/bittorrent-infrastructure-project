use bencode::BencodeConvertError;
use thiserror::Error;

use crate::message::error::ErrorMessage;

#[allow(clippy::module_name_repetitions)]
#[derive(Error, Debug)]
pub enum DhtError {
    #[error("Bencode error: {0}")]
    Bencode(#[from] BencodeConvertError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Node Sent An Invalid Message With Message Code {code}")]
    InvalidMessage { code: String },
    #[error("Node Sent Us An Invalid Response: {details}")]
    InvalidResponse { details: String },
    #[error("Node Sent Us An Unsolicited Response")]
    UnsolicitedResponse,
    #[error("Node Sent Us An Invalid Request Message With Code {msg:?} And Message {msg}")]
    InvalidRequest { msg: ErrorMessage<'static> },
}
