use std::io::Write;
use std::{io, u8};

use nom::{be_u8, IResult};

const BT_PROTOCOL: &'static [u8] = b"BitTorrent protocol";
const BT_PROTOCOL_LEN: u8 = 19;

/// `Protocol` information transmitted as part of the handshake.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Protocol {
    BitTorrent,
    Custom(Vec<u8>),
}

impl Protocol {
    /// Create a `Protocol` from the given bytes.
    pub fn from_bytes(bytes: &[u8]) -> IResult<&[u8], Protocol> {
        parse_protocol(bytes)
    }

    /// Write the `Protocol` out to the given writer.
    pub fn write_bytes<W>(&self, mut writer: W) -> io::Result<()>
    where
        W: Write,
    {
        let (len, bytes) = match self {
            &Protocol::BitTorrent => (BT_PROTOCOL_LEN as usize, &BT_PROTOCOL[..]),
            &Protocol::Custom(ref prot) => (prot.len(), &prot[..]),
        };

        try!(writer.write_all(&[len as u8][..]));
        try!(writer.write_all(bytes));

        Ok(())
    }

    /// Get the legth of the given protocol (does not include the length byte).
    pub fn write_len(&self) -> usize {
        match self {
            &Protocol::BitTorrent => BT_PROTOCOL_LEN as usize,
            &Protocol::Custom(ref custom) => custom.len(),
        }
    }
}

fn parse_protocol(bytes: &[u8]) -> IResult<&[u8], Protocol> {
    parse_real_protocol(bytes)
}

#[allow(unreachable_patterns, unused)]
fn parse_real_protocol(bytes: &[u8]) -> IResult<&[u8], Protocol> {
    switch!(bytes, parse_raw_protocol,
        // TODO: Move back to using constant here, for now, MIR compiler error occurs
        b"BitTorrent protocol" => value!(Protocol::BitTorrent) |
        custom                 => value!(Protocol::Custom(custom.to_vec()))
    )
}

fn parse_raw_protocol(bytes: &[u8]) -> IResult<&[u8], &[u8]> {
    do_parse!(bytes,
        length:       be_u8         >>
        raw_protocol: take!(length) >>
        (raw_protocol)
    )
}
