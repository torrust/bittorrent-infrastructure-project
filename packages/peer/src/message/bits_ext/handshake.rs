use std::collections::HashMap;
use std::io::{self, Write};
use std::mem;
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
    /// Create a new `ExtendedMessageBuilder`.
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

    /// Set our client identification in the message.
    #[must_use]
    pub fn with_our_id(mut self, id: Option<String>) -> ExtendedMessageBuilder {
        self.our_id = id;
        self
    }

    /// Set the given `ExtendedType` to map to the given value.
    #[must_use]
    pub fn with_extended_type(mut self, ext_type: ExtendedType, opt_value: Option<u8>) -> ExtendedMessageBuilder {
        if let Some(value) = opt_value {
            self.id_map.insert(ext_type, value);
        } else {
            self.id_map.remove(&ext_type);
        }
        self
    }

    /// Set our tcp port.
    #[must_use]
    pub fn with_our_tcp_port(mut self, tcp: Option<u16>) -> ExtendedMessageBuilder {
        self.our_tcp_port = tcp;
        self
    }

    /// Set the ip address that we see them as.
    #[must_use]
    pub fn with_their_ip(mut self, ip: Option<IpAddr>) -> ExtendedMessageBuilder {
        self.their_ip = ip;
        self
    }

    /// Set our ipv6 address.
    #[must_use]
    pub fn with_our_ipv6_addr(mut self, ipv6: Option<Ipv6Addr>) -> ExtendedMessageBuilder {
        self.our_ipv6_addr = ipv6;
        self
    }

    /// Set our ipv4 address.
    #[must_use]
    pub fn with_our_ipv4_addr(mut self, ipv4: Option<Ipv4Addr>) -> ExtendedMessageBuilder {
        self.our_ipv4_addr = ipv4;
        self
    }

    /// Set the maximum number of queued requests we support.
    #[must_use]
    pub fn with_max_requests(mut self, max_requests: Option<i64>) -> ExtendedMessageBuilder {
        self.our_max_requests = max_requests;
        self
    }

    /// Set the info dictionary metadata size.
    #[must_use]
    pub fn with_metadata_size(mut self, metadata_size: Option<i64>) -> ExtendedMessageBuilder {
        self.metadata_size = metadata_size;
        self
    }

    /// Set a custom entry in the message with the given dictionary key.
    #[must_use]
    pub fn with_custom_entry(mut self, key: String, opt_value: Option<BencodeMut<'static>>) -> ExtendedMessageBuilder {
        if let Some(value) = opt_value {
            self.custom_entries.insert(key, value);
        } else {
            self.custom_entries.remove(&key);
        }
        self
    }

    /// Build an `ExtendedMessage` with the current options.
    #[must_use]
    pub fn build(self) -> ExtendedMessage {
        ExtendedMessage::from_builder(self)
    }
}

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

// Terminology is written as if we were receiving the message. Example: Our ip is
// the ip that the sender sees us as. So if were sending this message, it would be
// the ip we see the client as.

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
    /// Create an `ExtendedType` from the given identifier.
    #[must_use]
    pub fn from_id(id: &str) -> ExtendedType {
        match id {
            UT_METADATA_ID => ExtendedType::UtMetadata,
            UT_PEX_ID => ExtendedType::UtPex,
            custom => ExtendedType::Custom(custom.to_string()),
        }
    }

    /// Retrieve the message id corresponding to the given `ExtendedType`.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            &ExtendedType::UtMetadata => UT_METADATA_ID,
            &ExtendedType::UtPex => UT_PEX_ID,
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
    /// Create an `ExtendedMessage` from an `ExtendedMessageBuilder`.
    #[must_use]
    pub fn from_builder(mut builder: ExtendedMessageBuilder) -> ExtendedMessage {
        let mut custom_entries = HashMap::new();
        mem::swap(&mut custom_entries, &mut builder.custom_entries);

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

    /// Parse an `ExtendedMessage` from some raw bencode of the given length.
    pub fn parse_bytes(_input: (), mut bytes: Bytes, len: u32) -> IResult<(), io::Result<ExtendedMessage>> {
        let cast_len = message::u32_to_usize(len);

        if bytes.len() >= cast_len {
            let raw_bencode = bytes.split_to(cast_len);
            let clone_raw_bencode = raw_bencode.clone();

            let res_extended_message = BencodeRef::decode(&raw_bencode, BDecodeOpt::default())
                .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))
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
                        raw_bencode: clone_raw_bencode,
                    })
                });

            IResult::Done((), res_extended_message)
        } else {
            IResult::Incomplete(Needed::Size(cast_len - bytes.len()))
        }
    }

    /// Write the `ExtendedMessage` out to the given writer.
    ///
    /// # Errors
    ///
    /// It will return an IP error if unable to write the bytes.
    ///
    /// # Panics
    ///
    /// It would panic if the bencode size it too large.
    pub fn write_bytes<W>(&self, mut writer: W) -> io::Result<()>
    where
        W: Write,
    {
        let real_length = 2 + self.bencode_size();
        message::write_length_id_pair(
            &mut writer,
            real_length.try_into().unwrap(),
            Some(bits_ext::EXTENDED_MESSAGE_ID),
        )?;

        writer.write_all(&[bits_ext::EXTENDED_MESSAGE_HANDSHAKE_ID]);

        writer.write_all(self.raw_bencode.as_ref())
    }

    /// Get the size of the bencode portion of this message.
    pub fn bencode_size(&self) -> usize {
        self.raw_bencode.len()
    }

    /// Query for the id corresponding to the given `ExtendedType`.
    pub fn query_id(&self, ext_type: &ExtendedType) -> Option<u8> {
        self.id_map.get(ext_type).copied()
    }

    /// Retrieve our id from the message.
    pub fn our_id(&self) -> Option<&str> {
        self.our_id.as_deref()
    }

    /// Retrieve our tcp port from the message.
    pub fn our_tcp_port(&self) -> Option<u16> {
        self.our_tcp_port
    }

    /// Retrieve their ip address from the message.
    pub fn their_ip(&self) -> Option<IpAddr> {
        self.their_ip
    }

    /// Retrieve our ipv6 address from the message.
    pub fn our_ipv6_addr(&self) -> Option<Ipv6Addr> {
        self.our_ipv6_addr
    }

    /// Retrieve our ipv4 address from the message.
    pub fn our_ipv4_addr(&self) -> Option<Ipv4Addr> {
        self.our_ipv4_addr
    }

    /// Retrieve our max queued requests from the message.
    pub fn our_max_requests(&self) -> Option<i64> {
        self.our_max_requests
    }

    /// Retrieve the info dictionary metadata size from the message.
    pub fn metadata_size(&self) -> Option<i64> {
        self.metadata_size
    }

    /// Retrieve a raw `BencodeRef` representing the current message.
    ///
    /// # Panics
    ///
    /// It would panic if unable to decode the bencode.
    pub fn bencode_ref(&self) -> BencodeRef<'_> {
        // We already verified that this is valid bencode
        BencodeRef::decode(&self.raw_bencode, BDecodeOpt::default()).unwrap()
    }
}
