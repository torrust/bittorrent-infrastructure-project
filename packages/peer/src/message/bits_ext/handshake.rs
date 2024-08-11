use std::collections::HashMap;
use std::io::Write as _;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use bencode::{ben_bytes, ben_int, BConvert, BDecodeOpt, BMutAccess, BencodeMut, BencodeRef};
use bytes::{Bytes, BytesMut};
use nom::{IResult, Needed};
use util::convert;

use crate::message;
use crate::message::{bencode_util, bits_ext};

/// Builder type for an `ExtendedMessage`.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct ExtendedMessageBuilder {
    id_map: HashMap<ExtendedType, u8>,
    our_id: Option<String>,
    our_tcp_port: Option<u16>,
    their_ip: Option<IpAddr>,
    our_ipv6_addr: Option<Ipv6Addr>,
    our_ipv4_addr: Option<Ipv4Addr>,
    our_max_requests: Option<i64>,
    metadata_size: Option<i64>,
    custom_entries: HashMap<String, BencodeMut<'static>>,
}

impl ExtendedMessageBuilder {
    /// Creates a new `ExtendedMessageBuilder`.
    ///
    /// # Returns
    ///
    /// A new `ExtendedMessageBuilder` instance.
    #[must_use]
    pub fn new() -> ExtendedMessageBuilder {
        ExtendedMessageBuilder {
            id_map: HashMap::new(),
            our_id: None,
            our_tcp_port: None,
            their_ip: None,
            our_ipv6_addr: None,
            our_ipv4_addr: None,
            our_max_requests: None,
            metadata_size: None,
            custom_entries: HashMap::new(),
        }
    }

    /// Sets our client identification in the message.
    ///
    /// # Parameters
    ///
    /// - `id`: The client identification string.
    ///
    /// # Returns
    ///
    /// The updated `ExtendedMessageBuilder`.
    #[must_use]
    pub fn with_our_id(mut self, id: Option<String>) -> ExtendedMessageBuilder {
        self.our_id = id;
        self
    }

    /// Sets the given `ExtendedType` to map to the given value.
    ///
    /// # Parameters
    ///
    /// - `ext_type`: The extended type.
    /// - `opt_value`: The optional value to map to the extended type.
    ///
    /// # Returns
    ///
    /// The updated `ExtendedMessageBuilder`.
    #[must_use]
    pub fn with_extended_type(mut self, ext_type: ExtendedType, opt_value: Option<u8>) -> ExtendedMessageBuilder {
        if let Some(value) = opt_value {
            self.id_map.insert(ext_type, value);
        } else {
            self.id_map.remove(&ext_type);
        }
        self
    }

    /// Sets our TCP port.
    ///
    /// # Parameters
    ///
    /// - `tcp`: The TCP port.
    ///
    /// # Returns
    ///
    /// The updated `ExtendedMessageBuilder`.
    #[must_use]
    pub fn with_our_tcp_port(mut self, tcp: Option<u16>) -> ExtendedMessageBuilder {
        self.our_tcp_port = tcp;
        self
    }

    /// Sets the IP address that we see them as.
    ///
    /// # Parameters
    ///
    /// - `ip`: The IP address.
    ///
    /// # Returns
    ///
    /// The updated `ExtendedMessageBuilder`.
    #[must_use]
    pub fn with_their_ip(mut self, ip: Option<IpAddr>) -> ExtendedMessageBuilder {
        self.their_ip = ip;
        self
    }

    /// Sets our IPv6 address.
    ///
    /// # Parameters
    ///
    /// - `ipv6`: The IPv6 address.
    ///
    /// # Returns
    ///
    /// The updated `ExtendedMessageBuilder`.
    #[must_use]
    pub fn with_our_ipv6_addr(mut self, ipv6: Option<Ipv6Addr>) -> ExtendedMessageBuilder {
        self.our_ipv6_addr = ipv6;
        self
    }

    /// Sets our IPv4 address.
    ///
    /// # Parameters
    ///
    /// - `ipv4`: The IPv4 address.
    ///
    /// # Returns
    ///
    /// The updated `ExtendedMessageBuilder`.
    #[must_use]
    pub fn with_our_ipv4_addr(mut self, ipv4: Option<Ipv4Addr>) -> ExtendedMessageBuilder {
        self.our_ipv4_addr = ipv4;
        self
    }

    /// Sets the maximum number of queued requests we support.
    ///
    /// # Parameters
    ///
    /// - `max_requests`: The maximum number of queued requests.
    ///
    /// # Returns
    ///
    /// The updated `ExtendedMessageBuilder`.
    #[must_use]
    pub fn with_max_requests(mut self, max_requests: Option<i64>) -> ExtendedMessageBuilder {
        self.our_max_requests = max_requests;
        self
    }

    /// Sets the info dictionary metadata size.
    ///
    /// # Parameters
    ///
    /// - `metadata_size`: The metadata size.
    ///
    /// # Returns
    ///
    /// The updated `ExtendedMessageBuilder`.
    #[must_use]
    pub fn with_metadata_size(mut self, metadata_size: Option<i64>) -> ExtendedMessageBuilder {
        self.metadata_size = metadata_size;
        self
    }

    /// Sets a custom entry in the message with the given dictionary key.
    ///
    /// # Parameters
    ///
    /// - `key`: The dictionary key.
    /// - `opt_value`: The optional value to set for the key.
    ///
    /// # Returns
    ///
    /// The updated `ExtendedMessageBuilder`.
    #[must_use]
    pub fn with_custom_entry(mut self, key: String, opt_value: Option<BencodeMut<'static>>) -> ExtendedMessageBuilder {
        if let Some(value) = opt_value {
            self.custom_entries.insert(key, value);
        } else {
            self.custom_entries.remove(&key);
        }
        self
    }

    /// Builds an `ExtendedMessage` with the current options.
    ///
    /// # Returns
    ///
    /// The built `ExtendedMessage`.
    #[must_use]
    pub fn build(self) -> ExtendedMessage {
        ExtendedMessage::from_builder(self)
    }
}

/// Encodes the builder's options into a bencode format.
///
/// # Parameters
///
/// - `builder`: The `ExtendedMessageBuilder` instance.
/// - `custom_entries`: A map of custom entries.
///
/// # Returns
///
/// A vector of bytes representing the bencoded data.
fn bencode_from_builder(builder: &ExtendedMessageBuilder, mut custom_entries: HashMap<String, BencodeMut<'static>>) -> Vec<u8> {
    let opt_our_ip = builder.their_ip.map(|their_ip| match their_ip {
        IpAddr::V4(ipv4_addr) => convert::ipv4_to_bytes_be(ipv4_addr).to_vec(),
        IpAddr::V6(ipv6_addr) => convert::ipv6_to_bytes_be(ipv6_addr).to_vec(),
    });
    let opt_client_ipv6_addr = builder.our_ipv6_addr.map(convert::ipv6_to_bytes_be);
    let opt_client_ipv4_addr = builder.our_ipv4_addr.map(convert::ipv4_to_bytes_be);

    let mut root_map = BencodeMut::new_dict();
    let mut ben_id_map = BencodeMut::new_dict();

    {
        let root_map_access = root_map.dict_mut().unwrap();

        {
            let ben_id_map_access = ben_id_map.dict_mut().unwrap();
            for (ext_id, &value) in &builder.id_map {
                ben_id_map_access.insert(ext_id.id().as_bytes().into(), ben_int!(i64::from(value)));
            }
        }

        root_map_access.insert(bencode_util::ID_MAP_KEY.into(), ben_id_map);

        for (key, value) in custom_entries.drain() {
            root_map_access.insert(key.into_bytes().into(), value);
        }

        builder
            .our_id
            .as_ref()
            .map(|client_id| root_map_access.insert(bencode_util::CLIENT_ID_KEY.into(), ben_bytes!(&client_id[..])));
        builder
            .our_tcp_port
            .map(|tcp_port| root_map_access.insert(bencode_util::CLIENT_TCP_PORT_KEY.into(), ben_int!(i64::from(tcp_port))));
        opt_our_ip.map(|our_ip| root_map_access.insert(bencode_util::OUR_IP_KEY.into(), ben_bytes!(our_ip)));
        opt_client_ipv6_addr.as_ref().map(|client_ipv6_addr| {
            root_map_access.insert(bencode_util::CLIENT_IPV6_ADDR_KEY.into(), ben_bytes!(&client_ipv6_addr[..]))
        });
        opt_client_ipv4_addr.as_ref().map(|client_ipv4_addr| {
            root_map_access.insert(bencode_util::CLIENT_IPV4_ADDR_KEY.into(), ben_bytes!(&client_ipv4_addr[..]))
        });
        builder.our_max_requests.map(|client_max_requests| {
            root_map_access.insert(bencode_util::CLIENT_MAX_REQUESTS_KEY.into(), ben_int!(client_max_requests))
        });
        builder
            .metadata_size
            .map(|metadata_size| root_map_access.insert(bencode_util::METADATA_SIZE_KEY.into(), ben_int!(metadata_size)));
    }

    root_map.encode()
}

// ----------------------------------------------------------------------------//

// Terminology is written as if we were receiving the message. Example: Our IP is
// the IP that the sender sees us as. So if we're sending this message, it would be
// the IP we see the client as.

const ROOT_ERROR_KEY: &str = "ExtendedMessage";

const UT_METADATA_ID: &str = "ut_metadata";
const UT_PEX_ID: &str = "ut_pex";

/// Enumeration of extended types activated via `ExtendedMessage`.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum ExtendedType {
    UtMetadata,
    UtPex,
    Custom(String),
}

impl ExtendedType {
    /// Creates an `ExtendedType` from the given identifier.
    ///
    /// # Parameters
    ///
    /// - `id`: The identifier string.
    ///
    /// # Returns
    ///
    /// An `ExtendedType` instance corresponding to the identifier.
    #[must_use]
    pub fn from_id(id: &str) -> ExtendedType {
        match id {
            UT_METADATA_ID => ExtendedType::UtMetadata,
            UT_PEX_ID => ExtendedType::UtPex,
            custom => ExtendedType::Custom(custom.to_string()),
        }
    }

    /// Retrieves the message id corresponding to the given `ExtendedType`.
    ///
    /// # Returns
    ///
    /// The message id as a string slice.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            ExtendedType::UtMetadata => UT_METADATA_ID,
            ExtendedType::UtPex => UT_PEX_ID,
            ExtendedType::Custom(id) => id,
        }
    }
}

/// Message for notifying peers of extensions we support.
///
/// See `http://www.bittorrent.org/beps/bep_0010.html`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtendedMessage {
    id_map: HashMap<ExtendedType, u8>,
    our_id: Option<String>,
    our_tcp_port: Option<u16>,
    their_ip: Option<IpAddr>,
    our_ipv6_addr: Option<Ipv6Addr>,
    our_ipv4_addr: Option<Ipv4Addr>,
    our_max_requests: Option<i64>,
    metadata_size: Option<i64>,
    raw_bencode: Bytes,
}

impl ExtendedMessage {
    /// Creates an `ExtendedMessage` from an `ExtendedMessageBuilder`.
    ///
    /// # Parameters
    ///
    /// - `builder`: The `ExtendedMessageBuilder` instance.
    ///
    /// # Returns
    ///
    /// An `ExtendedMessage` instance.
    #[must_use]
    pub fn from_builder(mut builder: ExtendedMessageBuilder) -> ExtendedMessage {
        let mut custom_entries = HashMap::new();
        std::mem::swap(&mut custom_entries, &mut builder.custom_entries);

        let encoded_bytes = bencode_from_builder(&builder, custom_entries);
        let mut raw_bencode = BytesMut::with_capacity(encoded_bytes.len());
        raw_bencode.extend_from_slice(&encoded_bytes);

        ExtendedMessage {
            id_map: builder.id_map,
            our_id: builder.our_id,
            our_tcp_port: builder.our_tcp_port,
            their_ip: builder.their_ip,
            our_ipv6_addr: builder.our_ipv6_addr,
            our_ipv4_addr: builder.our_ipv4_addr,
            our_max_requests: builder.our_max_requests,
            metadata_size: builder.metadata_size,
            raw_bencode: raw_bencode.freeze(),
        }
    }

    /// Parses an `ExtendedMessage` from some raw bencode of the given length.
    ///
    /// # Parameters
    ///
    /// - `bytes`: The byte slice to parse.
    /// - `len`: The length of the bencode data.
    ///
    /// # Returns
    ///
    /// An `IResult` containing the remaining byte slice and an `io::Result` with the parsed `ExtendedMessage`.
    ///
    /// # Errors
    ///
    /// This function will return an error if the byte slice cannot be parsed into an `ExtendedMessage`.
    pub fn parse_bytes(bytes: &[u8], len: u32) -> IResult<&[u8], std::io::Result<ExtendedMessage>> {
        let cast_len = message::u32_to_usize(len);

        if bytes.len() >= cast_len {
            let (raw_bencode, _) = bytes.split_at(cast_len);

            let res_extended_message = BencodeRef::decode(raw_bencode, BDecodeOpt::default())
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err.to_string()))
                .and_then(|bencode| {
                    let ben_dict = bencode_util::CONVERT.convert_dict(&bencode, ROOT_ERROR_KEY)?;

                    let id_map = bencode_util::parse_id_map(ben_dict);
                    let our_id = bencode_util::parse_client_id(ben_dict);
                    let our_tcp_port = bencode_util::parse_client_tcp_port(ben_dict);
                    let their_ip = bencode_util::parse_our_ip(ben_dict);
                    let our_ipv6_addr = bencode_util::parse_client_ipv6_addr(ben_dict);
                    let our_ipv4_addr = bencode_util::parse_client_ipv4_addr(ben_dict);
                    let our_max_requests = bencode_util::parse_client_max_requests(ben_dict);
                    let metadata_size = bencode_util::parse_metadata_size(ben_dict);

                    Ok(ExtendedMessage {
                        id_map,
                        our_id,
                        our_tcp_port,
                        their_ip,
                        our_ipv6_addr,
                        our_ipv4_addr,
                        our_max_requests,
                        metadata_size,
                        raw_bencode: Bytes::copy_from_slice(raw_bencode),
                    })
                });

            // Clone the remaining bytes to avoid returning a reference to the parameter
            IResult::Ok((bytes, res_extended_message))
        } else {
            Err(nom::Err::Incomplete(Needed::new(cast_len - bytes.len())))
        }
    }

    /// Writes the `ExtendedMessage` out to the given writer.
    ///
    /// # Parameters
    ///
    /// - `writer`: The writer to which the bytes will be written.
    ///
    /// # Errors
    ///
    /// This function will return an error if unable to write the bytes.
    ///
    /// # Panics
    ///
    /// This function will panic if the bencode size is too large.
    pub fn write_bytes<W>(&self, mut writer: W) -> std::io::Result<usize>
    where
        W: std::io::Write,
    {
        let real_length: u32 = (self.bencode_size() + 2)
            .try_into()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Unsupported, e))?;

        let len = message::write_length_id_pair(&mut writer, real_length, Some(bits_ext::EXTENDED_MESSAGE_ID))?;

        let () = writer.write_all(&[bits_ext::EXTENDED_MESSAGE_HANDSHAKE_ID])?;
        let () = writer.write_all(self.raw_bencode.as_ref())?;
        Ok(real_length as usize + len)
    }

    /// Gets the size of the bencode portion of this message.
    ///
    /// # Returns
    ///
    /// The size of the bencode portion in bytes.
    pub fn bencode_size(&self) -> usize {
        self.raw_bencode.len()
    }

    /// Queries for the id corresponding to the given `ExtendedType`.
    ///
    /// # Parameters
    ///
    /// - `ext_type`: The extended type.
    ///
    /// # Returns
    ///
    /// An optional u8 representing the id.
    pub fn query_id(&self, ext_type: &ExtendedType) -> Option<u8> {
        self.id_map.get(ext_type).copied()
    }

    /// Retrieves our id from the message.
    ///
    /// # Returns
    ///
    /// An optional string slice representing our id.
    pub fn our_id(&self) -> Option<&str> {
        self.our_id.as_deref()
    }

    /// Retrieves our TCP port from the message.
    ///
    /// # Returns
    ///
    /// An optional u16 representing our TCP port.
    pub fn our_tcp_port(&self) -> Option<u16> {
        self.our_tcp_port
    }

    /// Retrieves their IP address from the message.
    ///
    /// # Returns
    ///
    /// An optional `IpAddr` representing their IP address.
    pub fn their_ip(&self) -> Option<IpAddr> {
        self.their_ip
    }

    /// Retrieves our IPv6 address from the message.
    ///
    /// # Returns
    ///
    /// An optional `Ipv6Addr` representing our IPv6 address.
    pub fn our_ipv6_addr(&self) -> Option<Ipv6Addr> {
        self.our_ipv6_addr
    }

    /// Retrieves our IPv4 address from the message.
    ///
    /// # Returns
    ///
    /// An optional `Ipv4Addr` representing our IPv4 address.
    pub fn our_ipv4_addr(&self) -> Option<Ipv4Addr> {
        self.our_ipv4_addr
    }

    /// Retrieves our max queued requests from the message.
    ///
    /// # Returns
    ///
    /// An optional i64 representing our max queued requests.
    pub fn our_max_requests(&self) -> Option<i64> {
        self.our_max_requests
    }

    /// Retrieves the info dictionary metadata size from the message.
    ///
    /// # Returns
    ///
    /// An optional i64 representing the metadata size.
    pub fn metadata_size(&self) -> Option<i64> {
        self.metadata_size
    }

    /// Retrieves a raw `BencodeRef` representing the current message.
    ///
    /// # Panics
    ///
    /// This function will panic if unable to decode the bencode.
    ///
    /// # Returns
    ///
    /// A `BencodeRef` representing the current message.
    pub fn bencode_ref(&self) -> BencodeRef<'_> {
        // We already verified that this is valid bencode
        BencodeRef::decode(&self.raw_bencode, BDecodeOpt::default()).unwrap()
    }
}
