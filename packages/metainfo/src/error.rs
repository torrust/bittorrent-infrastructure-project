//! Errors for torrent file building and parsing.

use std::io;

use bencode::{BencodeConvertError, BencodeParseError};
use error_chain::error_chain;
use walkdir;

error_chain! {
    types {
        ParseError, ParseErrorKind, ParseResultEx, ParseResult;
    }

    foreign_links {
        Io(io::Error);
        Dir(walkdir::Error);
        BencodeConvert(BencodeConvertError);
        BencodeParse(BencodeParseError);
    }

    errors {
        MissingData {
            details: String
        } {
            description("Missing Data Detected In File")
            display("Missing Data Detected In File: {}", details)
        }
    }
}
