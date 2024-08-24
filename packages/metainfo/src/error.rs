//! Errors for torrent file building and parsing.

use bencode::{BencodeConvertError, BencodeParseError};
use thiserror::Error;
use walkdir;

#[allow(clippy::module_name_repetitions)]
#[derive(Error, Debug)]
pub enum ParseError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Directory error: {0}")]
    Dir(#[from] walkdir::Error),

    #[error("Bencode conversion error: {0}")]
    BencodeConvert(#[from] BencodeConvertError),

    #[error("Bencode parse error: {0}")]
    BencodeParse(#[from] BencodeParseError),

    #[error("Missing Data Detected In File: {details}")]
    MissingData { details: String },
}
