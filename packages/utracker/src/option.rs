//! Messaging primitives for announce options.

use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::io::Write as _;

use byteorder::WriteBytesExt;
use nom::branch::alt;
use nom::bytes::complete::{tag, take};
use nom::combinator::{eof, map};
use nom::multi::length_data;
use nom::number::complete::be_u8;
use nom::sequence::tuple;
use nom::IResult;
use tracing::instrument;

const END_OF_OPTIONS_BYTE: u8 = 0x00;
const NO_OPERATION_BYTE: u8 = 0x01;
const URL_DATA_BYTE: u8 = 0x02;

/// Trait for supplying optional information in an `AnnounceRequest`.
#[allow(clippy::module_name_repetitions)]
pub trait AnnounceOption<'a>: Sized {
    /// Byte specifying what option this is.
    fn option_byte() -> u8;

    /// Length of the associated option data.
    fn option_length(&self) -> usize;

    /// Reads the option content from the given bytes.
    fn read_option(bytes: &'a [u8]) -> Option<Self>;

    /// Writes the option payload into the given buffer.
    fn write_option(&self, buffer: &mut [u8]);
}

// ----------------------------------------------------------------------------//

/// Set of announce options used to provide trackers with extra information.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct AnnounceOptions<'a> {
    raw_options: HashMap<u8, Cow<'a, [u8]>>,
}

impl<'a> AnnounceOptions<'a> {
    /// Create a new set of `AnnounceOptions`.
    #[must_use]
    pub fn new() -> AnnounceOptions<'a> {
        AnnounceOptions {
            raw_options: HashMap::new(),
        }
    }

    /// Parse a set of `AnnounceOptions` from the given bytes.
    ///
    /// # Errors
    ///
    /// It will return an error when unable to parse the bytes.
    pub fn from_bytes(bytes: &'a [u8]) -> IResult<&'a [u8], AnnounceOptions<'a>> {
        let mut raw_options = HashMap::new();

        let (remaining, _) = parse_options(bytes, &mut raw_options)?;
        Ok((remaining, AnnounceOptions { raw_options }))
    }

    /// Write the `AnnounceOptions` to the given writer.
    ///
    /// # Errors
    ///
    /// It would return an IO Error if unable to write the bytes.
    ///
    /// # Panics
    ///
    /// It would panic if the chunk length is too large.
    #[instrument(skip(self, writer), err)]
    pub fn write_bytes<W>(&self, mut writer: W) -> std::io::Result<()>
    where
        W: std::io::Write,
    {
        tracing::trace!("writing {} options", self.raw_options.len());
        for (byte, content) in &self.raw_options {
            for content_chunk in content.chunks(u8::MAX as usize) {
                let content_chunk_len: u8 = content_chunk.len().try_into().unwrap();

                writer.write_u8(*byte)?;
                writer.write_u8(content_chunk_len)?;
                writer.write_all(content_chunk)?;
            }
        }

        // If we can fit it in, include the option terminating byte, otherwise as per the
        // spec, we can leave it out since we are assuming this is the end of the packet.
        match writer.write_u8(END_OF_OPTIONS_BYTE) {
            Ok(()) => Ok(()),
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WriteZero {
                    tracing::trace!("no space to write ending marker");
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Search for and construct the given `AnnounceOption` from the current `AnnounceOptions`.
    ///
    /// Returns None if the option is not found or it failed to read from the given bytes.
    #[must_use]
    pub fn get<O>(&'a self) -> Option<O>
    where
        O: AnnounceOption<'a>,
    {
        self.raw_options
            .get(&O::option_byte())
            .and_then(|bytes| O::read_option(bytes))
    }

    /// Add an `AnnounceOption` to the current set of `AnnounceOptions`.
    ///
    /// Any existing option with a matching option byte will be replaced.
    pub fn insert<O>(&mut self, option: &O)
    where
        O: AnnounceOption<'a>,
    {
        let mut bytes = vec![0u8; option.option_length()];
        option.write_option(&mut bytes[..]);

        self.insert_bytes(O::option_byte(), bytes);
    }

    /// Create an owned version of `AnnounceOptions`.
    #[must_use]
    pub fn to_owned(&self) -> AnnounceOptions<'static> {
        let mut options = AnnounceOptions::new();

        for (&key, value) in &self.raw_options {
            options.insert_bytes(key, (*value).to_vec());
        }

        options
    }

    fn insert_bytes(&mut self, byte: u8, contents: Vec<u8>) {
        self.raw_options.insert(byte, Cow::Owned(contents));
    }
}

/// Parse the options in the byte slice and store them in the option map.
fn parse_options<'a>(bytes: &'a [u8], option_map: &mut HashMap<u8, Cow<'a, [u8]>>) -> IResult<&'a [u8], bool> {
    let mut curr_bytes = bytes;
    let mut eof = false;

    while !eof {
        let parse_result = alt((parse_end_option, parse_no_option, |input| {
            parse_user_option(input, option_map)
        }))(curr_bytes);

        match parse_result {
            Ok((new_bytes, found_eof)) => {
                eof = found_eof;
                curr_bytes = new_bytes;
            }
            Err(e) => {
                return Err(e);
            }
        };
    }

    Ok((curr_bytes, eof))
}

/// Parse an end of buffer or the end of option byte.
fn parse_end_option(input: &[u8]) -> IResult<&[u8], bool> {
    map(alt((eof, tag([END_OF_OPTIONS_BYTE]))), |_| true)(input)
}

/// Parse a noop byte.
fn parse_no_option(input: &[u8]) -> IResult<&[u8], bool> {
    map(tag([NO_OPERATION_BYTE]), |_| false)(input)
}

/// Parse a user defined option.
fn parse_user_option<'a>(input: &'a [u8], option_map: &mut HashMap<u8, Cow<'a, [u8]>>) -> IResult<&'a [u8], bool> {
    let (input, (option_byte, option_contents)) = tuple((be_u8, length_data(be_u8)))(input)?;

    match option_map.entry(option_byte) {
        Entry::Occupied(mut occ) => {
            occ.get_mut().to_mut().extend_from_slice(option_contents);
        }
        Entry::Vacant(vac) => {
            vac.insert(Cow::Borrowed(option_contents));
        }
    };

    Ok((input, false))
}

/// Concatenated PATH and QUERY of a UDP tracker URL.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct URLDataOption<'a> {
    url_data: &'a [u8],
}

impl<'a> URLDataOption<'a> {
    /// Create a new `URLDataOption` from the given bytes.
    #[must_use]
    pub fn new(url_data: &'a [u8]) -> URLDataOption<'a> {
        URLDataOption { url_data }
    }
}

impl<'a> AnnounceOption<'a> for URLDataOption<'a> {
    fn option_byte() -> u8 {
        URL_DATA_BYTE
    }

    fn option_length(&self) -> usize {
        self.url_data.len()
    }

    fn read_option(bytes: &'a [u8]) -> Option<URLDataOption<'a>> {
        Some(URLDataOption { url_data: bytes })
    }

    fn write_option(&self, mut buffer: &mut [u8]) {
        buffer.write_all(self.url_data).unwrap();
    }
}

#[cfg(test)]
mod tests {

    use std::io::Write as _;
    use std::sync::Once;

    use nom::IResult;
    use tracing::level_filters::LevelFilter;

    use super::{AnnounceOptions, URLDataOption};

    #[allow(dead_code)]
    pub static INIT: Once = Once::new();

    #[allow(dead_code)]
    pub fn tracing_stderr_init(filter: LevelFilter) {
        let builder = tracing_subscriber::fmt()
            .with_max_level(filter)
            .with_ansi(true)
            .with_writer(std::io::stderr);

        builder.pretty().with_file(true).init();

        tracing::info!("Logging initialized");
    }

    #[test]
    fn positive_write_eof_option() {
        INIT.call_once(|| {
            tracing_stderr_init(LevelFilter::INFO);
        });

        let mut received = [];

        let options = AnnounceOptions::new();
        options.write_bytes(&mut received[..]).unwrap();

        let expected = [];

        assert_eq!(&received[..], &expected[..]);
    }

    #[test]
    fn positive_write_empty_option() {
        let mut received = Vec::new();

        let options = AnnounceOptions::new();
        options.write_bytes(&mut received).unwrap();

        let expected = [super::END_OF_OPTIONS_BYTE];

        assert_eq!(&received[..], &expected[..]);
    }

    #[test]
    fn positive_write_single_option() {
        let mut received = Vec::new();

        let option = URLDataOption::new(b"AA");

        let mut options = AnnounceOptions::new();
        options.insert(&option);
        options.write_bytes(&mut received).unwrap();

        let expected = [super::URL_DATA_BYTE, 2, b'A', b'A', super::END_OF_OPTIONS_BYTE];

        assert_eq!(&received[..], &expected[..]);
    }

    #[test]
    fn positive_write_chunked_option() {
        let mut received = Vec::new();

        let mut option_content = [0u8; 256];
        option_content[255] = 123;

        let option = URLDataOption::new(&option_content);
        let mut options = AnnounceOptions::new();
        options.insert(&option);
        options.write_bytes(&mut received).unwrap();

        let mut expected = Vec::new();
        expected.write_all(&[super::URL_DATA_BYTE, 255]).unwrap();
        expected.write_all(option_content.chunks(255).next().unwrap()).unwrap();
        expected.write_all(&[super::URL_DATA_BYTE, 1]).unwrap();
        expected.write_all(option_content.chunks(255).nth(1).unwrap()).unwrap();
        expected.write_all(&[super::END_OF_OPTIONS_BYTE]).unwrap();

        assert_eq!(&received[..], &expected[..]);
    }

    #[test]
    fn positive_parse_empty_option() {
        let bytes = [];

        let received = AnnounceOptions::from_bytes(&bytes);
        let expected = AnnounceOptions::new();

        assert_eq!(received, IResult::Ok((&b""[..], expected)));
    }

    #[test]
    fn positive_parse_noop_option() {
        let bytes = [super::NO_OPERATION_BYTE];

        let received = AnnounceOptions::from_bytes(&bytes);
        let expected = AnnounceOptions::new();

        assert_eq!(received, IResult::Ok((&b""[..], expected)));
    }

    #[test]
    fn positive_parse_end_option() {
        let bytes = [super::END_OF_OPTIONS_BYTE];

        let received = AnnounceOptions::from_bytes(&bytes);
        let expected = AnnounceOptions::new();

        assert_eq!(received, IResult::Ok((&b""[..], expected)));
    }

    #[test]
    fn positive_parse_url_data_eof() {
        let bytes = [super::URL_DATA_BYTE, 5, 0, 0, 0, 0, 0];
        let url_data_bytes = [0, 0, 0, 0, 0];

        let received = AnnounceOptions::from_bytes(&bytes);
        let mut expected = AnnounceOptions::new();

        let url_data = URLDataOption::new(&url_data_bytes);
        expected.insert(&url_data);

        assert_eq!(received, IResult::Ok((&b""[..], expected)));
    }

    #[test]
    fn positive_parse_url_data_end_of_options() {
        let bytes = [super::URL_DATA_BYTE, 5, 0, 0, 0, 0, 0, super::END_OF_OPTIONS_BYTE];
        let url_data_bytes = [0, 0, 0, 0, 0];

        let received = AnnounceOptions::from_bytes(&bytes);
        let mut expected = AnnounceOptions::new();

        let url_data = URLDataOption::new(&url_data_bytes);
        expected.insert(&url_data);

        assert_eq!(received, IResult::Ok((&b""[..], expected)));
    }

    #[test]
    fn positive_parse_url_data_noop_eof() {
        let bytes = [super::URL_DATA_BYTE, 5, 0, 0, 0, 0, 0, super::NO_OPERATION_BYTE];
        let url_data_bytes = [0, 0, 0, 0, 0];

        let received = AnnounceOptions::from_bytes(&bytes);
        let mut expected = AnnounceOptions::new();

        let url_data = URLDataOption::new(&url_data_bytes);
        expected.insert(&url_data);

        assert_eq!(received, IResult::Ok((&b""[..], expected)));
    }

    #[test]
    fn positive_parse_url_data_noop_end_of_options() {
        let bytes = [
            super::URL_DATA_BYTE,
            5,
            0,
            0,
            0,
            0,
            0,
            super::NO_OPERATION_BYTE,
            super::END_OF_OPTIONS_BYTE,
        ];
        let url_data_bytes = [0, 0, 0, 0, 0];

        let received = AnnounceOptions::from_bytes(&bytes);
        let mut expected = AnnounceOptions::new();

        let url_data = URLDataOption::new(&url_data_bytes);
        expected.insert(&url_data);

        assert_eq!(received, IResult::Ok((&b""[..], expected)));
    }

    #[test]
    fn positive_parse_url_data_single_chunk() {
        const NUM_BYTES: usize = u8::MAX as usize + 2;

        let mut bytes = [0u8; NUM_BYTES];
        bytes[0] = super::URL_DATA_BYTE;
        bytes[1] = u8::MAX;
        bytes[256] = 230;

        let received = AnnounceOptions::from_bytes(&bytes);
        let mut expected = AnnounceOptions::new();

        let url_data = URLDataOption::new(&bytes[2..]);
        expected.insert(&url_data);

        assert_eq!(received, IResult::Ok((&b""[..], expected)));
    }

    #[test]
    fn positive_parse_url_data_two_chunks() {
        const NUM_BYTES: usize = u8::MAX as usize + 2;

        let mut bytes = [0u8; 2 * NUM_BYTES];
        let mut url_data_bytes = Vec::new();
        {
            let bytes_one = &mut bytes[..NUM_BYTES];
            bytes_one[0] = super::URL_DATA_BYTE;
            bytes_one[1] = u8::MAX;
            bytes_one[256] = 230;

            url_data_bytes.extend_from_slice(&bytes_one[2..]);
        }
        {
            let bytes_two = &mut bytes[NUM_BYTES..];
            bytes_two[0] = super::URL_DATA_BYTE;
            bytes_two[1] = u8::MAX;
            bytes_two[256] = 210;

            url_data_bytes.extend_from_slice(&bytes_two[2..]);
        }

        let received = AnnounceOptions::from_bytes(&bytes);
        let mut expected = AnnounceOptions::new();

        let url_data = URLDataOption::new(&url_data_bytes[..]);
        expected.insert(&url_data);

        assert_eq!(received, IResult::Ok((&b""[..], expected)));
    }

    #[test]
    fn positive_parse_url_data_indivisible_chunks() {
        const NUM_BYTES: usize = u8::MAX as usize + 2;

        // Add an option tag, length, and a single byte as the payload to create an indivisible
        // chunk (not evenly divisible by u8::MAX) to see if it serializes correctly.
        let mut bytes = [0u8; NUM_BYTES + 3];
        let mut url_data_bytes = Vec::new();
        {
            let bytes_one = &mut bytes[..NUM_BYTES];
            bytes_one[0] = super::URL_DATA_BYTE;
            bytes_one[1] = u8::MAX;
            bytes_one[256] = 230;

            url_data_bytes.extend_from_slice(&bytes_one[2..]);
        }
        {
            let bytes_two = &mut bytes[NUM_BYTES..];
            bytes_two[0] = super::URL_DATA_BYTE;
            bytes_two[1] = 1;
            bytes_two[2] = 210;

            url_data_bytes.extend_from_slice(&bytes_two[2..]);
        }

        let received = AnnounceOptions::from_bytes(&bytes);
        let mut expected = AnnounceOptions::new();

        let url_data = URLDataOption::new(&url_data_bytes[..]);
        expected.insert(&url_data);

        assert_eq!(received, IResult::Ok((&b""[..], expected)));
    }

    #[test]
    fn negative_parse_url_data_incomplete() {
        let bytes = [super::URL_DATA_BYTE, 5, 0, 0];

        let received = AnnounceOptions::from_bytes(&bytes);

        assert!(received.is_err());
    }

    #[test]
    fn negative_parse_url_data_unterminated() {
        let bytes = [super::URL_DATA_BYTE, 5, 0, 0, 0, 0, 0, 60];

        let received = AnnounceOptions::from_bytes(&bytes);

        assert!(received.is_err());
    }
}
