#![allow(deprecated)]
//! Messaging primitives for announcing.

use std::io::Write as _;
use std::net::{Ipv4Addr, Ipv6Addr};

use byteorder::{BigEndian, WriteBytesExt};
use nom::branch::alt;
use nom::bytes::complete::{tag, take};
use nom::combinator::{map, value};
use nom::multi::count;
use nom::number::complete::{be_i32, be_i64, be_u16, be_u32, be_u8};
use nom::sequence::tuple;
use nom::IResult;
use tracing::instrument;
use util::bt::{self, InfoHash, PeerId};
use util::convert;

use crate::contact::CompactPeers;
use crate::option::AnnounceOptions;

const IMPLIED_IPV4_ID: [u8; 4] = [0u8; 4];
const IMPLIED_IPV6_ID: [u8; 16] = [0u8; 16];

const DEFAULT_NUM_WANT: i32 = -1;

const ANNOUNCE_NONE_EVENT: i32 = 0;
const ANNOUNCE_COMPLETED_EVENT: i32 = 1;
const ANNOUNCE_STARTED_EVENT: i32 = 2;
const ANNOUNCE_STOPPED_EVENT: i32 = 3;

/// Announce request sent from the client to the server.
///
/// IPv6 is supported but is [not standard](http://opentracker.blog.h3q.com/2007/12/28/the-ipv6-situation/).
#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnnounceRequest<'a> {
    info_hash: InfoHash,
    peer_id: PeerId,
    state: ClientState,
    ip: SourceIP,
    key: u32,
    num_want: DesiredPeers,
    port: u16,
    options: AnnounceOptions<'a>,
}

#[allow(clippy::too_many_arguments)]
impl<'a> AnnounceRequest<'a> {
    /// Create a new `AnnounceRequest`.
    #[instrument(skip())]
    #[must_use]
    pub fn new(
        hash: InfoHash,
        peer_id: PeerId,
        state: ClientState,
        ip: SourceIP,
        key: u32,
        num_want: DesiredPeers,
        port: u16,
        options: AnnounceOptions<'a>,
    ) -> AnnounceRequest<'a> {
        tracing::trace!("new announce request");
        AnnounceRequest {
            info_hash: hash,
            peer_id,
            state,
            ip,
            key,
            num_want,
            port,
            options,
        }
    }

    /// Construct an IPv4 `AnnounceRequest` from the given bytes.
    ///
    /// # Errors
    ///
    /// It will return an error when unable to parse the bytes.
    pub fn from_bytes_v4(bytes: &'a [u8]) -> IResult<&'a [u8], AnnounceRequest<'a>> {
        parse_request(bytes, SourceIP::from_bytes_v4)
    }

    /// Construct an IPv6 `AnnounceRequest` from the given bytes.
    ///
    /// # Errors
    ///
    /// It will return an error when unable to parse the bytes.
    pub fn from_bytes_v6(bytes: &'a [u8]) -> IResult<&'a [u8], AnnounceRequest<'a>> {
        parse_request(bytes, SourceIP::from_bytes_v6)
    }

    /// Write the `AnnounceRequest` to the given writer.
    ///
    /// # Errors
    ///
    /// It would return an IO error if unable to write the bytes.
    pub fn write_bytes<W>(&self, mut writer: W) -> std::io::Result<()>
    where
        W: std::io::Write,
    {
        writer.write_all(self.info_hash.as_ref())?;
        writer.write_all(self.peer_id.as_ref())?;

        self.state.write_bytes(&mut writer)?;
        self.ip.write_bytes(&mut writer)?;

        writer.write_u32::<BigEndian>(self.key)?;

        self.num_want.write_bytes(&mut writer)?;

        writer.write_u16::<BigEndian>(self.port)?;

        self.options.write_bytes(&mut writer)?;

        Ok(())
    }

    /// `InfoHash` of the current request.
    #[must_use]
    pub fn info_hash(&self) -> InfoHash {
        self.info_hash
    }

    /// `PeerId` of the current request.
    #[must_use]
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    /// State reported by the client in the given request.
    #[must_use]
    pub fn state(&self) -> ClientState {
        self.state
    }

    /// Source address to send the response to.
    #[must_use]
    pub fn source_ip(&self) -> SourceIP {
        self.ip
    }

    /// Unique key randomized by the client that the server can use.
    #[must_use]
    pub fn key(&self) -> u32 {
        self.key
    }

    /// Number of peers desired by the client.
    #[must_use]
    pub fn num_want(&self) -> DesiredPeers {
        self.num_want
    }

    /// Port to send the response to.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Set of `AnnounceOptions` supplied in the request.
    #[must_use]
    pub fn options(&self) -> &AnnounceOptions<'a> {
        &self.options
    }

    /// Create an owned version of `AnnounceRequest`.
    #[must_use]
    pub fn to_owned(&self) -> AnnounceRequest<'static> {
        // Do not call clone and simply switch out the AnnounceOptions as that would
        // unnecessarily allocate a HashMap with shallowly cloned Cow objects which
        // is superfluous.
        let owned_options = self.options.to_owned();

        AnnounceRequest {
            info_hash: self.info_hash,
            peer_id: self.peer_id,
            state: self.state,
            ip: self.ip,
            key: self.key,
            num_want: self.num_want,
            port: self.port,
            options: owned_options,
        }
    }
}

/// Parse an `AnnounceRequest` with the given `SourceIP` type constructor.
fn parse_request(bytes: &[u8], ip_type: fn(bytes: &[u8]) -> IResult<&[u8], SourceIP>) -> IResult<&[u8], AnnounceRequest<'_>> {
    let (bytes, (info_hash, peer_id, state, ip, key, num_want, port, options)) = tuple((
        map(take(bt::INFO_HASH_LEN), |bytes: &[u8]| InfoHash::from_hash(bytes).unwrap()),
        map(take(bt::PEER_ID_LEN), |bytes: &[u8]| PeerId::from_hash(bytes).unwrap()),
        ClientState::from_bytes,
        ip_type,
        be_u32,
        DesiredPeers::from_bytes,
        be_u16,
        AnnounceOptions::from_bytes,
    ))(bytes)?;

    Ok((
        bytes,
        AnnounceRequest::new(info_hash, peer_id, state, ip, key, num_want, port, options),
    ))
}

// ----------------------------------------------------------------------------//

/// Announce response sent from the server to the client.
#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnnounceResponse<'a> {
    interval: i32,
    leechers: i32,
    seeders: i32,
    peers: CompactPeers<'a>,
}

impl<'a> AnnounceResponse<'a> {
    /// Create a new `AnnounceResponse`
    #[must_use]
    pub fn new(interval: i32, leechers: i32, seeders: i32, peers: CompactPeers<'a>) -> AnnounceResponse<'a> {
        AnnounceResponse {
            interval,
            leechers,
            seeders,
            peers,
        }
    }

    /// Construct an IPv4 `AnnounceResponse` from the given bytes.
    ///
    /// # Errors
    ///
    /// It will return an error when unable to parse the bytes.
    pub fn from_bytes_v4(bytes: &'a [u8]) -> IResult<&'a [u8], AnnounceResponse<'a>> {
        parse_response(bytes, CompactPeers::from_bytes_v4)
    }

    /// Construct an IPv6 `AnnounceResponse` from the given bytes.
    ///
    /// # Errors
    ///
    /// It will return an error when unable to parse the bytes.
    pub fn from_bytes_v6(bytes: &'a [u8]) -> IResult<&'a [u8], AnnounceResponse<'a>> {
        parse_response(bytes, CompactPeers::from_bytes_v6)
    }

    /// Write the `AnnounceResponse` to the given writer.
    ///
    /// # Errors
    ///
    /// It would return an IO Error if unable to write the bytes.
    pub fn write_bytes<W>(&self, mut writer: W) -> std::io::Result<()>
    where
        W: std::io::Write,
    {
        writer.write_i32::<BigEndian>(self.interval)?;
        writer.write_i32::<BigEndian>(self.leechers)?;
        writer.write_i32::<BigEndian>(self.seeders)?;

        self.peers.write_bytes(&mut writer)?;

        Ok(())
    }

    /// Interval in seconds that clients should wait before re-announcing.
    #[must_use]
    pub fn interval(&self) -> i32 {
        self.interval
    }

    /// Number of leechers the tracker knows about for the torrent.
    #[must_use]
    pub fn leechers(&self) -> i32 {
        self.leechers
    }

    /// Number of seeders the tracker knows about for the torrent.
    #[must_use]
    pub fn seeders(&self) -> i32 {
        self.seeders
    }

    /// Peers the tracker knows about that are sharing the torrent.
    #[must_use]
    pub fn peers(&self) -> &CompactPeers<'a> {
        &self.peers
    }

    /// Create an owned version of `AnnounceResponse`.
    #[must_use]
    pub fn to_owned(&self) -> AnnounceResponse<'static> {
        let owned_peers = self.peers().to_owned();

        AnnounceResponse {
            interval: self.interval,
            leechers: self.leechers,
            seeders: self.seeders,
            peers: owned_peers,
        }
    }
}

/// Parse an `AnnounceResponse` with the given `CompactPeers` type constructor.
fn parse_response<'a>(
    bytes: &'a [u8],
    peers_type: fn(bytes: &'a [u8]) -> IResult<&'a [u8], CompactPeers<'a>>,
) -> IResult<&'a [u8], AnnounceResponse<'a>> {
    let (bytes, (interval, leechers, seeders, peers)) = tuple((be_i32, be_i32, be_i32, peers_type))(bytes)?;

    Ok((bytes, AnnounceResponse::new(interval, leechers, seeders, peers)))
}

// ----------------------------------------------------------------------------//

/// Announce state of a client reported to the server.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct ClientState {
    downloaded: i64,
    left: i64,
    uploaded: i64,
    event: AnnounceEvent,
}

impl ClientState {
    /// Create a new `ClientState`.
    #[must_use]
    #[instrument(skip())]
    pub fn new(bytes_downloaded: i64, bytes_left: i64, bytes_uploaded: i64, event: AnnounceEvent) -> ClientState {
        tracing::trace!("new client state");
        ClientState {
            downloaded: bytes_downloaded,
            left: bytes_left,
            uploaded: bytes_uploaded,
            event,
        }
    }

    /// Construct the `ClientState` from the given bytes.
    ///
    /// # Errors
    ///
    /// It will return an error when unable to parse the bytes.
    pub fn from_bytes(bytes: &[u8]) -> IResult<&[u8], ClientState> {
        parse_state(bytes)
    }

    /// Write the `ClientState` to the given writer.
    ///
    /// # Errors
    ///
    /// It would return an IO Error if unable to write the bytes.
    pub fn write_bytes<W>(&self, mut writer: W) -> std::io::Result<()>
    where
        W: std::io::Write,
    {
        writer.write_i64::<BigEndian>(self.downloaded)?;
        writer.write_i64::<BigEndian>(self.left)?;
        writer.write_i64::<BigEndian>(self.uploaded)?;

        self.event.write_bytes(&mut writer)?;

        Ok(())
    }

    /// Event reported by the client.
    #[must_use]
    pub fn event(&self) -> AnnounceEvent {
        self.event
    }

    /// Bytes left to be downloaded.
    #[must_use]
    pub fn bytes_left(&self) -> i64 {
        self.left
    }

    /// Bytes already uploaded.
    #[must_use]
    pub fn bytes_uploaded(&self) -> i64 {
        self.uploaded
    }

    /// Bytes already downloaded.
    #[must_use]
    pub fn bytes_downloaded(&self) -> i64 {
        self.downloaded
    }
}

fn parse_state(bytes: &[u8]) -> IResult<&[u8], ClientState> {
    let (bytes, (downloaded, left, uploaded, event)) = tuple((be_i64, be_i64, be_i64, AnnounceEvent::from_bytes))(bytes)?;

    Ok((bytes, ClientState::new(downloaded, left, uploaded, event)))
}

// ----------------------------------------------------------------------------//

/// Announce event of a client reported to the server.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AnnounceEvent {
    /// No event is reported.
    None,
    /// Torrent download has completed.
    Completed,
    /// Torrent download has started.
    Started,
    /// Torrent download has stopped.
    Stopped,
}

impl AnnounceEvent {
    /// Construct an `AnnounceEvent` from the given bytes.
    ///
    /// # Errors
    ///
    /// It will return an error when unable to parse the bytes.
    pub fn from_bytes(bytes: &[u8]) -> IResult<&[u8], AnnounceEvent> {
        parse_event(bytes)
    }

    /// Write the `AnnounceEvent` to the given writer.
    ///
    /// # Errors
    ///
    /// It would return an IO Error if unable to write the bytes.
    pub fn write_bytes<W>(&self, mut writer: W) -> std::io::Result<()>
    where
        W: std::io::Write,
    {
        writer.write_i32::<BigEndian>(self.as_id())?;

        Ok(())
    }

    /// Access the raw id of the current event.
    #[must_use]
    pub fn as_id(&self) -> i32 {
        match *self {
            AnnounceEvent::None => ANNOUNCE_NONE_EVENT,
            AnnounceEvent::Completed => ANNOUNCE_COMPLETED_EVENT,
            AnnounceEvent::Started => ANNOUNCE_STARTED_EVENT,
            AnnounceEvent::Stopped => ANNOUNCE_STOPPED_EVENT,
        }
    }
}

fn parse_event(bytes: &[u8]) -> IResult<&[u8], AnnounceEvent> {
    let (bytes, event_id) = be_i32(bytes)?;
    let event = match event_id {
        ANNOUNCE_NONE_EVENT => AnnounceEvent::None,
        ANNOUNCE_COMPLETED_EVENT => AnnounceEvent::Completed,
        ANNOUNCE_STARTED_EVENT => AnnounceEvent::Started,
        ANNOUNCE_STOPPED_EVENT => AnnounceEvent::Stopped,
        _ => return Err(nom::Err::Error(nom::error::Error::new(bytes, nom::error::ErrorKind::Switch))),
    };
    Ok((bytes, event))
}

// ----------------------------------------------------------------------------//

/// Client specified IP address to send the response to.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum SourceIP {
    /// Infer the IPv4 address from the sender address.
    ImpliedV4,
    /// Send the response to the given IPv4 address.
    ExplicitV4(Ipv4Addr),
    /// Infer the IPv6 address from the sender address.
    ImpliedV6,
    /// Send the response to the given IPv6 address.
    ExplicitV6(Ipv6Addr),
}

impl SourceIP {
    /// Construct the IPv4 `SourceIP` from the given bytes.
    ///
    /// # Errors
    ///
    /// It will return an error when unable to parse the bytes.
    pub fn from_bytes_v4(bytes: &[u8]) -> IResult<&[u8], SourceIP> {
        parse_preference_v4(bytes)
    }

    /// Construct the IPv6 `SourceIP` from the given bytes.
    ///
    /// # Errors
    ///
    /// It will return an error when unable to parse the bytes.
    pub fn from_bytes_v6(bytes: &[u8]) -> IResult<&[u8], SourceIP> {
        parse_preference_v6(bytes)
    }

    /// Write the `SourceIP` to the given writer.
    ///
    /// # Errors
    ///
    /// It would return an IO Error if unable to write the bytes.
    pub fn write_bytes<W>(&self, writer: W) -> std::io::Result<()>
    where
        W: std::io::Write,
    {
        match *self {
            SourceIP::ImpliedV4 => SourceIP::write_bytes_slice(writer, &IMPLIED_IPV4_ID[..]),
            SourceIP::ImpliedV6 => SourceIP::write_bytes_slice(writer, &IMPLIED_IPV6_ID[..]),
            SourceIP::ExplicitV4(addr) => SourceIP::write_bytes_slice(writer, &convert::ipv4_to_bytes_be(addr)[..]),
            SourceIP::ExplicitV6(addr) => SourceIP::write_bytes_slice(writer, &convert::ipv6_to_bytes_be(addr)[..]),
        }
    }

    /// Whether or not the source is an IPv6 address.
    #[must_use]
    pub fn is_ipv6(&self) -> bool {
        match *self {
            SourceIP::ExplicitV6(_) | SourceIP::ImpliedV6 => true,
            SourceIP::ImpliedV4 | SourceIP::ExplicitV4(_) => false,
        }
    }

    /// Whether or not the source is an IPv4 address.
    #[must_use]
    pub fn is_ipv4(&self) -> bool {
        !self.is_ipv6()
    }

    /// Write the given byte slice to the given writer.
    fn write_bytes_slice<W>(mut writer: W, bytes: &[u8]) -> std::io::Result<()>
    where
        W: std::io::Write,
    {
        writer.write_all(bytes)
    }
}

fn parse_preference_v4(bytes: &[u8]) -> IResult<&[u8], SourceIP> {
    let (bytes, ip) = alt((
        map(tag(&IMPLIED_IPV4_ID), |_| SourceIP::ImpliedV4),
        map(parse_ipv4, SourceIP::ExplicitV4),
    ))(bytes)?;
    Ok((bytes, ip))
}

fn parse_ipv4(bytes: &[u8]) -> IResult<&[u8], Ipv4Addr> {
    let (bytes, ip_bytes) = take(4usize)(bytes)?;
    let ip_array: [u8; 4] = ip_bytes.try_into().expect("slice with incorrect length");
    let ip = convert::bytes_be_to_ipv4(ip_array);
    Ok((bytes, ip))
}

fn parse_preference_v6(bytes: &[u8]) -> IResult<&[u8], SourceIP> {
    let (bytes, ip) = alt((
        map(tag(&IMPLIED_IPV6_ID), |_| SourceIP::ImpliedV6),
        map(parse_ipv6, SourceIP::ExplicitV6),
    ))(bytes)?;
    Ok((bytes, ip))
}

fn parse_ipv6(bytes: &[u8]) -> IResult<&[u8], Ipv6Addr> {
    let (bytes, ip_bytes) = take(16usize)(bytes)?;
    let ip_array: [u8; 16] = ip_bytes.try_into().expect("slice with incorrect length");
    let ip = convert::bytes_be_to_ipv6(ip_array);
    Ok((bytes, ip))
}

// ----------------------------------------------------------------------------//

/// Client desired number of peers to send in the response.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum DesiredPeers {
    /// Send the default number of peers.
    Default,
    /// Send a specific number of peers.
    Specified(i32),
}

impl DesiredPeers {
    /// Construct the `DesiredPeers` from the given bytes.
    ///
    /// # Errors
    ///
    /// It will return an error when unable to parse the bytes.
    pub fn from_bytes(bytes: &[u8]) -> IResult<&[u8], DesiredPeers> {
        parse_desired(bytes)
    }

    /// Write the `DesiredPeers` to the given writer.
    ///
    /// # Errors
    ///
    /// It would return an IO Error if unable to write the bytes.
    pub fn write_bytes<W>(&self, mut writer: W) -> std::io::Result<()>
    where
        W: std::io::Write,
    {
        let write_value = match self {
            DesiredPeers::Default => DEFAULT_NUM_WANT,
            DesiredPeers::Specified(count) => *count,
        };
        writer.write_i32::<BigEndian>(write_value)?;

        Ok(())
    }
}

fn parse_desired(bytes: &[u8]) -> IResult<&[u8], DesiredPeers> {
    let (bytes, num_want) = be_i32(bytes)?;
    let desired_peers = if num_want == DEFAULT_NUM_WANT {
        DesiredPeers::Default
    } else {
        DesiredPeers::Specified(num_want)
    };
    Ok((bytes, desired_peers))
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::net::{Ipv4Addr, Ipv6Addr};

    use byteorder::{BigEndian, WriteBytesExt};
    use nom::IResult;
    use util::bt::{InfoHash, PeerId};
    use util::convert;

    use super::{AnnounceEvent, AnnounceRequest, AnnounceResponse, ClientState, DesiredPeers, SourceIP};
    use crate::announce::{parse_ipv4, parse_ipv6};
    use crate::contact::{CompactPeers, CompactPeersV4, CompactPeersV6};
    use crate::option::AnnounceOptions;

    #[test]
    fn positive_write_request() {
        let mut received = Vec::new();

        let info_hash = [3, 4, 2, 3, 4, 3, 1, 6, 7, 56, 3, 234, 2, 3, 4, 3, 5, 6, 7, 8];
        let peer_id = [4, 2, 123, 23, 34, 5, 56, 2, 3, 4, 45, 6, 7, 8, 5, 6, 4, 56, 34, 42];
        let (downloaded, left, uploaded) = (123_908, 12_309_123, 123_123);
        let state = ClientState::new(downloaded, left, uploaded, AnnounceEvent::None);
        let ip = Ipv4Addr::new(127, 0, 0, 1);
        let key = 234_234;
        let num_want = 34;
        let port = 6969;
        let options = AnnounceOptions::new();
        let request = AnnounceRequest::new(
            info_hash.into(),
            peer_id.into(),
            state,
            SourceIP::ExplicitV4(ip),
            key,
            DesiredPeers::Specified(num_want),
            port,
            options.clone(),
        );

        request.write_bytes(&mut received).unwrap();

        let mut expected = Vec::new();
        expected.write_all(&info_hash).unwrap();
        expected.write_all(&peer_id).unwrap();
        expected.write_i64::<BigEndian>(downloaded).unwrap();
        expected.write_i64::<BigEndian>(left).unwrap();
        expected.write_i64::<BigEndian>(uploaded).unwrap();
        expected.write_i32::<BigEndian>(super::ANNOUNCE_NONE_EVENT).unwrap();
        expected.write_all(&convert::ipv4_to_bytes_be(ip)).unwrap();
        expected.write_u32::<BigEndian>(key).unwrap();
        expected.write_i32::<BigEndian>(num_want).unwrap();
        expected.write_u16::<BigEndian>(port).unwrap();
        options.write_bytes(&mut expected).unwrap();

        assert_eq!(&received[..], &expected[..]);
    }

    #[test]
    fn positive_write_response() {
        let mut received = Vec::new();

        let (interval, leechers, seeders) = (213_123, 3_423_423, 2_342_343);
        let mut peers = CompactPeersV4::new();
        peers.insert("127.0.0.1:2342".parse().unwrap());
        peers.insert("127.0.0.2:0".parse().unwrap());

        let response = AnnounceResponse::new(interval, leechers, seeders, CompactPeers::V4(peers.clone()));

        response.write_bytes(&mut received).unwrap();

        let mut expected = Vec::new();
        expected.write_i32::<BigEndian>(interval).unwrap();
        expected.write_i32::<BigEndian>(leechers).unwrap();
        expected.write_i32::<BigEndian>(seeders).unwrap();
        peers.write_bytes(&mut expected).unwrap();

        assert_eq!(&received[..], &expected[..]);
    }

    #[test]
    fn positive_write_state() {
        let mut received = Vec::new();

        let (downloaded, left, uploaded) = (123_908, 12_309_123, 123_123);
        let state = ClientState::new(downloaded, left, uploaded, AnnounceEvent::None);
        state.write_bytes(&mut received).unwrap();

        let mut expected = Vec::new();
        expected.write_i64::<BigEndian>(downloaded).unwrap();
        expected.write_i64::<BigEndian>(left).unwrap();
        expected.write_i64::<BigEndian>(uploaded).unwrap();
        expected.write_i32::<BigEndian>(super::ANNOUNCE_NONE_EVENT).unwrap();

        assert_eq!(&received[..], &expected[..]);
    }

    #[test]
    fn positive_write_none_event() {
        let mut received = Vec::new();

        let none_event = AnnounceEvent::None;
        none_event.write_bytes(&mut received).unwrap();

        let mut expected = Vec::new();
        expected.write_i32::<BigEndian>(super::ANNOUNCE_NONE_EVENT).unwrap();

        assert_eq!(&received[..], &expected[..]);
    }

    #[test]
    fn positive_write_completed_event() {
        let mut received = Vec::new();

        let none_event = AnnounceEvent::Completed;
        none_event.write_bytes(&mut received).unwrap();

        let mut expected = Vec::new();
        expected.write_i32::<BigEndian>(super::ANNOUNCE_COMPLETED_EVENT).unwrap();

        assert_eq!(&received[..], &expected[..]);
    }

    #[test]
    fn positive_write_started_event() {
        let mut received = Vec::new();

        let none_event = AnnounceEvent::Started;
        none_event.write_bytes(&mut received).unwrap();

        let mut expected = Vec::new();
        expected.write_i32::<BigEndian>(super::ANNOUNCE_STARTED_EVENT).unwrap();

        assert_eq!(&received[..], &expected[..]);
    }

    #[test]
    fn positive_write_stopped_event() {
        let mut received = Vec::new();

        let none_event = AnnounceEvent::Stopped;
        none_event.write_bytes(&mut received).unwrap();

        let mut expected = Vec::new();
        expected.write_i32::<BigEndian>(super::ANNOUNCE_STOPPED_EVENT).unwrap();

        assert_eq!(&received[..], &expected[..]);
    }

    #[test]
    fn positive_write_source_ipv4_implied() {
        let mut received = Vec::new();

        let implied_ip = SourceIP::ImpliedV4;
        implied_ip.write_bytes(&mut received).unwrap();

        let mut expected = Vec::new();
        expected.write_all(&super::IMPLIED_IPV4_ID).unwrap();

        assert_eq!(&received[..], &expected[..]);
    }

    #[test]
    fn positive_write_source_ipv6_implied() {
        let mut received = Vec::new();

        let implied_ip = SourceIP::ImpliedV6;
        implied_ip.write_bytes(&mut received).unwrap();

        let mut expected = Vec::new();
        expected.write_all(&super::IMPLIED_IPV6_ID).unwrap();

        assert_eq!(&received[..], &expected[..]);
    }

    #[test]
    fn positive_write_source_ipv4_explicit() {
        let mut received = Vec::new();

        let ip = Ipv4Addr::new(127, 0, 0, 1);
        let explicit_ip = SourceIP::ExplicitV4(ip);
        explicit_ip.write_bytes(&mut received).unwrap();

        let expected = convert::ipv4_to_bytes_be(ip);

        assert_eq!(&received[..], &expected[..]);
    }

    #[test]
    fn positive_write_source_ipv6_explicit() {
        let mut received = Vec::new();

        let ip = "ADBB:234A:55BD:FF34:3D3A:FFFF:234A:55BD".parse().unwrap(); // cspell:disable-line
        let explicit_ip = SourceIP::ExplicitV6(ip);
        explicit_ip.write_bytes(&mut received).unwrap();

        let expected = convert::ipv6_to_bytes_be(ip);

        assert_eq!(&received[..], &expected[..]);
    }

    #[test]
    fn positive_write_desired_peers_default() {
        let mut received = Vec::new();

        let desired_peers = DesiredPeers::Default;
        desired_peers.write_bytes(&mut received).unwrap();

        let mut expected = Vec::new();
        expected.write_i32::<BigEndian>(super::DEFAULT_NUM_WANT).unwrap();

        assert_eq!(&received[..], &expected[..]);
    }

    #[test]
    fn positive_write_desired_peers_specified() {
        let mut received = Vec::new();

        let desired_peers = DesiredPeers::Specified(500);
        desired_peers.write_bytes(&mut received).unwrap();

        let mut expected = Vec::new();
        expected.write_i32::<BigEndian>(500).unwrap();

        assert_eq!(&received[..], &expected[..]);
    }

    #[test]
    fn positive_parse_request_empty_options() {
        let info_hash = [0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1];
        let peer_id = [2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3];

        let (downloaded, left, uploaded) = (456_789, 283_465, 200_000);
        let state = ClientState::new(downloaded, left, uploaded, AnnounceEvent::Completed);

        let ip = SourceIP::ImpliedV4;
        let (key, num_want, port) = (255_123, -102_340, 1515);

        let mut bytes = Vec::new();
        bytes.write_all(&info_hash[..]).unwrap();
        bytes.write_all(&peer_id[..]).unwrap();
        bytes.write_i64::<BigEndian>(downloaded).unwrap();
        bytes.write_i64::<BigEndian>(left).unwrap();
        bytes.write_i64::<BigEndian>(uploaded).unwrap();
        bytes.write_i32::<BigEndian>(super::ANNOUNCE_COMPLETED_EVENT).unwrap();
        bytes.write_all(&super::IMPLIED_IPV4_ID).unwrap();
        bytes.write_u32::<BigEndian>(key).unwrap();
        bytes.write_i32::<BigEndian>(num_want).unwrap();
        bytes.write_u16::<BigEndian>(port).unwrap();

        let received = AnnounceRequest::from_bytes_v4(&bytes).unwrap().1;

        assert_eq!(received.info_hash(), InfoHash::from(info_hash));
        assert_eq!(received.peer_id(), PeerId::from(peer_id));
        assert_eq!(received.state(), state);
        assert_eq!(received.source_ip(), ip);
        assert_eq!(received.key(), key);
        assert_eq!(received.num_want(), DesiredPeers::Specified(num_want));
        assert_eq!(received.port(), port);
    }

    #[test]
    fn negative_parse_request_missing_key() {
        let info_hash = [0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1];
        let peer_id = [2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3, 2, 3];

        let (downloaded, left, uploaded) = (456_789, 283_465, 200_000);

        let (_, num_want, port) = (255_123, -102_340, 1515);

        let mut bytes = Vec::new();
        bytes.write_all(&info_hash[..]).unwrap();
        bytes.write_all(&peer_id[..]).unwrap();
        bytes.write_i64::<BigEndian>(downloaded).unwrap();
        bytes.write_i64::<BigEndian>(left).unwrap();
        bytes.write_i64::<BigEndian>(uploaded).unwrap();
        bytes.write_i32::<BigEndian>(super::ANNOUNCE_COMPLETED_EVENT).unwrap();
        bytes.write_all(&super::IMPLIED_IPV4_ID).unwrap();
        bytes.write_i32::<BigEndian>(num_want).unwrap();
        bytes.write_u16::<BigEndian>(port).unwrap();

        let received = AnnounceRequest::from_bytes_v4(&bytes);

        assert!(received.is_err());
    }

    #[test]
    fn positive_parse_response_empty_peers() {
        let (interval, leechers, seeders) = (34, 234, 0);

        let mut bytes = Vec::new();
        bytes.write_i32::<BigEndian>(interval).unwrap();
        bytes.write_i32::<BigEndian>(leechers).unwrap();
        bytes.write_i32::<BigEndian>(seeders).unwrap();

        let received_v4 = AnnounceResponse::from_bytes_v4(&bytes).unwrap().1;
        let received_v6 = AnnounceResponse::from_bytes_v6(&bytes).unwrap().1;

        let expected_v4 = AnnounceResponse::new(interval, leechers, seeders, CompactPeers::V4(CompactPeersV4::new()));
        let expected_v6 = AnnounceResponse::new(interval, leechers, seeders, CompactPeers::V6(CompactPeersV6::new()));

        assert_eq!(received_v4, expected_v4);
        assert_eq!(received_v6, expected_v6);
    }

    #[test]
    fn positive_parse_response_many_peers() {
        let (interval, leechers, seeders) = (34, 234, 0);

        let mut peers_v4 = CompactPeersV4::new();
        peers_v4.insert("127.0.0.1:3412".parse().unwrap());
        peers_v4.insert("10.0.0.1:2323".parse().unwrap());

        let mut peers_v6 = CompactPeersV6::new();
        peers_v6.insert("[ADBB:234A:55BD:FF34:3D3A:FFFF:234A:55BD]:3432".parse().unwrap()); // cspell:disable-line
        peers_v6.insert("[ADBB:234A::FF34:3D3A:FFFF:234A:55BD]:2222".parse().unwrap()); // cspell:disable-line

        let mut bytes = Vec::new();
        bytes.write_i32::<BigEndian>(interval).unwrap();
        bytes.write_i32::<BigEndian>(leechers).unwrap();
        bytes.write_i32::<BigEndian>(seeders).unwrap();

        let mut bytes_v4 = bytes.clone();
        peers_v4.write_bytes(&mut bytes_v4).unwrap();

        let mut bytes_v6 = bytes.clone();
        peers_v6.write_bytes(&mut bytes_v6).unwrap();

        let received_v4 = AnnounceResponse::from_bytes_v4(&bytes_v4).unwrap().1;
        let received_v6 = AnnounceResponse::from_bytes_v6(&bytes_v6).unwrap().1;

        let expected_v4 = AnnounceResponse::new(interval, leechers, seeders, CompactPeers::V4(peers_v4));
        let expected_v6 = AnnounceResponse::new(interval, leechers, seeders, CompactPeers::V6(peers_v6));

        assert_eq!(received_v4, expected_v4);
        assert_eq!(received_v6, expected_v6);
    }

    #[test]
    fn positive_parse_state() {
        let (downloaded, left, uploaded) = (202_340, 52_340, 5_043);

        let mut bytes = Vec::new();
        bytes.write_i64::<BigEndian>(downloaded).unwrap();
        bytes.write_i64::<BigEndian>(left).unwrap();
        bytes.write_i64::<BigEndian>(uploaded).unwrap();
        bytes.write_i32::<BigEndian>(super::ANNOUNCE_NONE_EVENT).unwrap();

        let received = ClientState::from_bytes(&bytes).unwrap().1;
        let expected = ClientState::new(downloaded, left, uploaded, AnnounceEvent::None);

        assert_eq!(received, expected);
    }

    #[test]
    fn negative_parse_incomplete_state() {
        let (downloaded, left, uploaded) = (202_340, 52_340, 5_043);

        let mut bytes = Vec::new();
        bytes.write_i64::<BigEndian>(downloaded).unwrap();
        bytes.write_i64::<BigEndian>(left).unwrap();
        bytes.write_i64::<BigEndian>(uploaded).unwrap();

        let received = ClientState::from_bytes(&bytes);

        assert!(received.is_err());
    }

    #[test]
    fn positive_parse_none_event() {
        let mut bytes = Vec::new();
        bytes.write_i32::<BigEndian>(super::ANNOUNCE_NONE_EVENT).unwrap();

        let received = AnnounceEvent::from_bytes(&bytes).unwrap().1;
        let expected = AnnounceEvent::None;

        assert_eq!(received, expected);
    }

    #[test]
    fn positive_parse_completed_event() {
        let mut bytes = Vec::new();
        bytes.write_i32::<BigEndian>(super::ANNOUNCE_COMPLETED_EVENT).unwrap();

        let received = AnnounceEvent::from_bytes(&bytes).unwrap().1;
        let expected = AnnounceEvent::Completed;

        assert_eq!(received, expected);
    }

    #[test]
    fn positive_parse_started_event() {
        let mut bytes = Vec::new();
        bytes.write_i32::<BigEndian>(super::ANNOUNCE_STARTED_EVENT).unwrap();

        let received = AnnounceEvent::from_bytes(&bytes).unwrap().1;
        let expected = AnnounceEvent::Started;

        assert_eq!(received, expected);
    }

    #[test]
    fn negative_parse_no_event() {
        let bytes = [1, 2, 3, 4];

        let received = AnnounceEvent::from_bytes(&bytes);

        assert!(received.is_err());
    }

    #[test]
    fn positive_parse_stopped_event() {
        let mut bytes = Vec::new();
        bytes.write_i32::<BigEndian>(super::ANNOUNCE_STOPPED_EVENT).unwrap();

        let received = AnnounceEvent::from_bytes(&bytes).unwrap().1;
        let expected = AnnounceEvent::Stopped;

        assert_eq!(received, expected);
    }

    #[test]
    fn positive_parse_implied_v4_source() {
        let mut bytes = Vec::new();
        bytes.write_all(&super::IMPLIED_IPV4_ID).unwrap();

        let received = SourceIP::from_bytes_v4(&bytes).unwrap().1;
        let expected = SourceIP::ImpliedV4;

        assert_eq!(received, expected);
    }

    #[test]
    fn positive_parse_explicit_v4_source() {
        let bytes = [127, 0, 0, 1];

        let received = SourceIP::from_bytes_v4(&bytes).unwrap().1;
        let expected = SourceIP::ExplicitV4(Ipv4Addr::new(127, 0, 0, 1));

        assert_eq!(received, expected);
    }

    #[test]
    fn positive_parse_implied_v6_source() {
        let mut bytes = Vec::new();
        bytes.write_all(&super::IMPLIED_IPV6_ID).unwrap();

        let received = SourceIP::from_bytes_v6(&bytes).unwrap().1;
        let expected = SourceIP::ImpliedV6;

        assert_eq!(received, expected);
    }

    #[test]
    fn positive_parse_explicit_v6_source() {
        let ip = "ADBB:234A:55BD:FF34:3D3A:FFFF:234A:55BD".parse().unwrap(); // cspell:disable-line
        let bytes = convert::ipv6_to_bytes_be(ip);

        let received = SourceIP::from_bytes_v6(&bytes).unwrap().1;
        let expected = SourceIP::ExplicitV6(ip);

        assert_eq!(received, expected);
    }

    #[test]
    fn negative_parse_incomplete_v4_source() {
        let bytes = [0, 0];

        let received = SourceIP::from_bytes_v4(&bytes);

        assert!(received.is_err());
    }

    #[test]
    fn negative_parse_incomplete_v6_source() {
        let bytes = [0, 0, 0, 0];

        let received = SourceIP::from_bytes_v6(&bytes);

        assert!(received.is_err());
    }

    #[test]
    fn negative_parse_empty_v4_source() {
        let bytes = [];

        let received = SourceIP::from_bytes_v4(&bytes);

        assert!(received.is_err());
    }

    #[test]
    fn negative_parse_empty_v6_source() {
        let bytes = [];

        let received = SourceIP::from_bytes_v6(&bytes);

        assert!(received.is_err());
    }

    #[test]
    fn positive_parse_desired_peers_default() {
        let default_bytes = convert::four_bytes_to_array(u32::MAX);

        let received = DesiredPeers::from_bytes(&default_bytes).unwrap().1;
        let expected = DesiredPeers::Default;

        assert_eq!(received, expected);
    }

    #[test]
    fn positive_parse_desired_peers_specified() {
        let specified_bytes = convert::four_bytes_to_array(50);

        let received = DesiredPeers::from_bytes(&specified_bytes).unwrap().1;
        let expected = DesiredPeers::Specified(50);

        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_ipv4() {
        // Valid IPv4 address
        let bytes = [192, 168, 0, 1];
        let expected_ip = Ipv4Addr::new(192, 168, 0, 1);
        let (remaining, parsed_ip) = parse_ipv4(&bytes).unwrap();
        assert_eq!(remaining, &[]);
        assert_eq!(parsed_ip, expected_ip);

        // Invalid IPv4 address (not enough bytes)
        let bytes = [192, 168];
        let result = parse_ipv4(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_ipv6() {
        // Valid IPv6 address
        let bytes = [
            0x20, 0x01, 0x0d, 0xb8, 0x85, 0xa3, 0x00, 0x00, 0x00, 0x00, 0x8a, 0x2e, 0x03, 0x70, 0x73, 0x34,
        ];
        let expected_ip = Ipv6Addr::new(0x2001, 0x0db8, 0x85a3, 0x0000, 0x0000, 0x8a2e, 0x0370, 0x7334);
        let (remaining, parsed_ip) = parse_ipv6(&bytes).unwrap();
        assert_eq!(remaining, &[]);
        assert_eq!(parsed_ip, expected_ip);

        // Invalid IPv6 address (not enough bytes)
        let bytes = [0x20, 0x01, 0x0d, 0xb8];
        let result = parse_ipv6(&bytes);
        assert!(result.is_err());
    }
}
