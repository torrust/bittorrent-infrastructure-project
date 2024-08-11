//! Messaging primitives for server errors.

use std::borrow::Cow;
use std::io::Write as _;

use nom::bytes::complete::take;
use nom::character::complete::not_line_ending;
use nom::combinator::map_res;
use nom::sequence::terminated;
use nom::IResult;
use thiserror::Error;

/// Error reported by the server and sent to the client.
#[allow(clippy::module_name_repetitions)]
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub struct ErrorResponse<'a> {
    message: Cow<'a, str>,
}

impl<'a> std::fmt::Display for ErrorResponse<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Server Error: {}", self.message)
    }
}

impl<'a> ErrorResponse<'a> {
    /// Create a new `ErrorResponse`.
    #[must_use]
    pub fn new(message: &'a str) -> ErrorResponse<'a> {
        ErrorResponse {
            message: Cow::Borrowed(message),
        }
    }

    /// Construct an `ErrorResponse` from the given bytes.
    ///
    /// # Errors
    ///
    /// It will return an error when unable to parse the bytes.
    pub fn from_bytes(bytes: &'a [u8]) -> IResult<&'a [u8], ErrorResponse<'a>> {
        let (remaining, message) = map_res(terminated(not_line_ending, take(0usize)), std::str::from_utf8)(bytes)?;
        Ok((remaining, ErrorResponse::new(message)))
    }

    /// Write the `ErrorResponse` to the given writer.
    ///
    /// # Errors
    ///
    /// It would return an IO Error if unable to write the bytes.
    pub fn write_bytes<W>(&self, mut writer: W) -> std::io::Result<()>
    where
        W: std::io::Write,
    {
        writer.write_all(self.message.as_bytes())?;

        Ok(())
    }

    /// Message describing the error that occurred.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Create an owned version of the `ErrorResponse`.
    #[must_use]
    pub fn to_owned(&self) -> ErrorResponse<'static> {
        ErrorResponse {
            message: Cow::Owned((*self.message).to_owned()),
        }
    }
}
