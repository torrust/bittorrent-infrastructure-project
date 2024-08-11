//! Messaging primitives for responses.

use std::io::Write as _;

use byteorder::{BigEndian, WriteBytesExt};
use nom::combinator::map;
use nom::number::complete::{be_u32, be_u64};
use nom::sequence::tuple;
use nom::IResult;

use crate::announce::AnnounceResponse;
use crate::contact::CompactPeers;
use crate::error::ErrorResponse;
use crate::scrape::ScrapeResponse;

/// Error action ids only occur in responses.
const ERROR_ACTION_ID: u32 = 3;

/// Enumerates all types of responses that can be received from a tracker.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub enum ResponseType<'a> {
    Connect(u64),
    Announce(AnnounceResponse<'a>),
    Scrape(ScrapeResponse<'a>),
    Error(ErrorResponse<'a>),
}

impl<'a> ResponseType<'a> {
    /// Create an owned version of the `ResponseType`.
    #[must_use]
    pub fn to_owned(&self) -> ResponseType<'static> {
        match self {
            &ResponseType::Connect(id) => ResponseType::Connect(id),
            ResponseType::Announce(res) => ResponseType::Announce(res.to_owned()),
            ResponseType::Scrape(res) => ResponseType::Scrape(res.to_owned()),
            ResponseType::Error(err) => ResponseType::Error(err.to_owned()),
        }
    }
}

/// `TrackerResponse` which encapsulates any response sent from a tracker.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct TrackerResponse<'a> {
    transaction_id: u32,
    response_type: ResponseType<'a>,
}

impl<'a> TrackerResponse<'a> {
    /// Create a new `TrackerResponse`.
    #[must_use]
    pub fn new(trans_id: u32, res_type: ResponseType<'a>) -> TrackerResponse<'a> {
        TrackerResponse {
            transaction_id: trans_id,
            response_type: res_type,
        }
    }

    /// Create a new `TrackerResponse` from the given bytes.
    ///
    /// # Errors
    ///
    /// It will return an error when unable to parse the bytes.
    pub fn from_bytes(bytes: &'a [u8]) -> IResult<&'a [u8], TrackerResponse<'a>> {
        parse_response(bytes)
    }

    /// Write the `TrackerResponse` to the given writer.
    ///
    /// # Errors
    ///
    /// It would return an IO Error if unable to write the bytes.
    pub fn write_bytes<W>(&self, mut writer: W) -> std::io::Result<()>
    where
        W: std::io::Write,
    {
        match self.response_type() {
            &ResponseType::Connect(id) => {
                writer.write_u32::<BigEndian>(crate::CONNECT_ACTION_ID)?;
                writer.write_u32::<BigEndian>(self.transaction_id())?;

                writer.write_u64::<BigEndian>(id)?;
            }
            ResponseType::Announce(req) => {
                let action_id = match req.peers() {
                    CompactPeers::V4(_) => crate::ANNOUNCE_IPV4_ACTION_ID,
                    CompactPeers::V6(_) => crate::ANNOUNCE_IPV6_ACTION_ID,
                };

                writer.write_u32::<BigEndian>(action_id)?;
                writer.write_u32::<BigEndian>(self.transaction_id())?;

                req.write_bytes(writer)?;
            }
            ResponseType::Scrape(req) => {
                writer.write_u32::<BigEndian>(crate::SCRAPE_ACTION_ID)?;
                writer.write_u32::<BigEndian>(self.transaction_id())?;

                req.write_bytes(writer)?;
            }
            ResponseType::Error(err) => {
                writer.write_u32::<BigEndian>(ERROR_ACTION_ID)?;
                writer.write_u32::<BigEndian>(self.transaction_id())?;

                err.write_bytes(writer)?;
            }
        };

        Ok(())
    }

    /// Transaction ID supplied with a response to uniquely identify a request.
    #[must_use]
    pub fn transaction_id(&self) -> u32 {
        self.transaction_id
    }

    /// Actual type of response that this `TrackerResponse` represents.
    #[must_use]
    pub fn response_type(&self) -> &ResponseType<'a> {
        &self.response_type
    }

    /// Create an owned version of the `TrackerResponse`.
    #[must_use]
    pub fn to_owned(&self) -> TrackerResponse<'static> {
        TrackerResponse {
            transaction_id: self.transaction_id,
            response_type: self.response_type().to_owned(),
        }
    }
}

fn parse_response(bytes: &[u8]) -> IResult<&[u8], TrackerResponse<'_>> {
    let (remaining, (action_id, transaction_id)) = tuple((be_u32, be_u32))(bytes)?;

    match action_id {
        crate::CONNECT_ACTION_ID => {
            let (remaining, connection_id) = be_u64(remaining)?;
            Ok((
                remaining,
                TrackerResponse::new(transaction_id, ResponseType::Connect(connection_id)),
            ))
        }
        crate::ANNOUNCE_IPV4_ACTION_ID => {
            let (remaining, ann_res) = AnnounceResponse::from_bytes_v4(remaining)?;
            Ok((
                remaining,
                TrackerResponse::new(transaction_id, ResponseType::Announce(ann_res)),
            ))
        }
        crate::SCRAPE_ACTION_ID => {
            let (remaining, scr_res) = ScrapeResponse::from_bytes(remaining)?;
            Ok((remaining, TrackerResponse::new(transaction_id, ResponseType::Scrape(scr_res))))
        }
        ERROR_ACTION_ID => {
            let (remaining, err_res) = ErrorResponse::from_bytes(remaining)?;
            Ok((remaining, TrackerResponse::new(transaction_id, ResponseType::Error(err_res))))
        }
        crate::ANNOUNCE_IPV6_ACTION_ID => {
            let (remaining, ann_res) = AnnounceResponse::from_bytes_v6(remaining)?;
            Ok((
                remaining,
                TrackerResponse::new(transaction_id, ResponseType::Announce(ann_res)),
            ))
        }
        _ => Err(nom::Err::Error(nom::error::Error::new(
            remaining,
            nom::error::ErrorKind::Switch,
        ))),
    }
}
