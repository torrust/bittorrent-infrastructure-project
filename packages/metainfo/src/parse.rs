use bencode::{BConvert, BDictAccess, BListAccess, BRefAccess, BencodeConvertError};

use crate::error::ParseError;

/// Struct implemented the `BencodeConvert` trait for decoding the metainfo file.
struct MetainfoConverter;

impl BConvert for MetainfoConverter {
    type Error = ParseError;

    fn handle_error(&self, error: BencodeConvertError) -> ParseError {
        error.into()
    }
}

/// Global instance for our conversion struct.
const CONVERT: MetainfoConverter = MetainfoConverter;

/// Used as an error key to refer to the root bencode object.
pub const ROOT_ERROR_KEY: &[u8] = b"root";

/// Keys found within the root dictionary of a metainfo file.
pub const ANNOUNCE_LIST_KEY: &[u8] = b"announce-list";
pub const ANNOUNCE_URL_KEY: &[u8] = b"announce";
pub const CREATION_DATE_KEY: &[u8] = b"creation date";
pub const COMMENT_KEY: &[u8] = b"comment";
pub const CREATED_BY_KEY: &[u8] = b"created by";
pub const ENCODING_KEY: &[u8] = b"encoding";
pub const INFO_KEY: &[u8] = b"info";

/// Keys found within the info dictionary of a metainfo file.
pub const PIECE_LENGTH_KEY: &[u8] = b"piece length";
pub const PIECES_KEY: &[u8] = b"pieces";
pub const PRIVATE_KEY: &[u8] = b"private";
pub const NAME_KEY: &[u8] = b"name";
pub const FILES_KEY: &[u8] = b"files";

/// Keys found within the files dictionary of a metainfo file.
pub const LENGTH_KEY: &[u8] = b"length";
pub const MD5SUM_KEY: &[u8] = b"md5sum";
pub const PATH_KEY: &[u8] = b"path";

/// Parses the root bencode as a dictionary.
#[allow(clippy::module_name_repetitions)]
pub fn parse_root_dict<B>(root_bencode: &B) -> Result<&dyn BDictAccess<B::BKey, B::BType>, ParseError>
where
    B: BRefAccess,
{
    CONVERT.convert_dict(root_bencode, ROOT_ERROR_KEY)
}

/// Parses the announce list from the root dictionary.
#[allow(clippy::module_name_repetitions)]
pub fn parse_announce_list<B>(root_dict: &dyn BDictAccess<B::BKey, B>) -> Option<&dyn BListAccess<B>>
where
    B: BRefAccess<BType = B>,
{
    CONVERT.lookup_and_convert_list(root_dict, ANNOUNCE_LIST_KEY).ok()
}

/// Converts list of lists to vec of vecs
pub fn convert_announce_list<B>(list: &dyn BListAccess<B>) -> Vec<Vec<String>>
where
    B: BRefAccess<BType = B>,
{
    list.into_iter()
        .filter_map(bencode::BRefAccess::list)
        .map(|entry| {
            entry
                .into_iter()
                .filter_map(bencode::BRefAccess::str)
                .map(String::from)
                .collect()
        })
        .collect()
}

/// Parses the announce url from the root dictionary.
#[allow(clippy::module_name_repetitions)]
pub fn parse_announce_url<'a, B>(root_dict: &'a dyn BDictAccess<B::BKey, B>) -> Option<&'a str>
where
    B: BRefAccess + 'a,
{
    CONVERT.lookup_and_convert_str(root_dict, ANNOUNCE_URL_KEY).ok()
}

/// Parses the creation date from the root dictionary.
#[allow(clippy::module_name_repetitions)]
pub fn parse_creation_date<B>(root_dict: &dyn BDictAccess<B::BKey, B>) -> Option<i64>
where
    B: BRefAccess,
{
    CONVERT.lookup_and_convert_int(root_dict, CREATION_DATE_KEY).ok()
}

/// Parses the comment from the root dictionary.
#[allow(clippy::module_name_repetitions)]
pub fn parse_comment<'a, B>(root_dict: &'a dyn BDictAccess<B::BKey, B>) -> Option<&'a str>
where
    B: BRefAccess + 'a,
{
    CONVERT.lookup_and_convert_str(root_dict, COMMENT_KEY).ok()
}

/// Parses the created by from the root dictionary.
#[allow(clippy::module_name_repetitions)]
pub fn parse_created_by<'a, B>(root_dict: &'a dyn BDictAccess<B::BKey, B>) -> Option<&'a str>
where
    B: BRefAccess + 'a,
{
    CONVERT.lookup_and_convert_str(root_dict, CREATED_BY_KEY).ok()
}

/// Parses the encoding from the root dictionary.
#[allow(clippy::module_name_repetitions)]
pub fn parse_encoding<'a, B>(root_dict: &'a dyn BDictAccess<B::BKey, B>) -> Option<&'a str>
where
    B: BRefAccess + 'a,
{
    CONVERT.lookup_and_convert_str(root_dict, ENCODING_KEY).ok()
}

/// Parses the info dictionary from the root dictionary.
#[allow(clippy::module_name_repetitions)]
pub fn parse_info_bencode<B>(root_dict: &dyn BDictAccess<B::BKey, B>) -> Result<&B, ParseError>
where
    B: BRefAccess,
{
    CONVERT.lookup(root_dict, INFO_KEY)
}

// ----------------------------------------------------------------------------//

/// Parses the piece length from the info dictionary.
#[allow(clippy::module_name_repetitions)]
pub fn parse_piece_length<B>(info_dict: &dyn BDictAccess<B::BKey, B>) -> Result<u64, ParseError>
where
    B: BRefAccess,
{
    CONVERT
        .lookup_and_convert_int(info_dict, PIECE_LENGTH_KEY)
        .map(|len| len.try_into().unwrap())
}

/// Parses the pieces from the info dictionary.
#[allow(clippy::module_name_repetitions)]
pub fn parse_pieces<'a, B>(info_dict: &'a dyn BDictAccess<B::BKey, B>) -> Result<&'a [u8], ParseError>
where
    B: BRefAccess + 'a,
{
    CONVERT.lookup_and_convert_bytes(info_dict, PIECES_KEY)
}

/// Parses the private flag from the info dictionary.
#[allow(clippy::module_name_repetitions)]
pub fn parse_private<B>(info_dict: &dyn BDictAccess<B::BKey, B>) -> Option<bool>
where
    B: BRefAccess,
{
    CONVERT.lookup_and_convert_int(info_dict, PRIVATE_KEY).ok().map(|p| p == 1)
}

/// Parses the name from the info dictionary.
#[allow(clippy::module_name_repetitions)]
pub fn parse_name<'a, B>(info_dict: &'a dyn BDictAccess<B::BKey, B>) -> Result<&'a str, ParseError>
where
    B: BRefAccess + 'a,
{
    CONVERT.lookup_and_convert_str(info_dict, NAME_KEY)
}

/// Parses the files list from the info dictionary.
#[allow(clippy::module_name_repetitions)]
pub fn parse_files_list<B>(info_dict: &dyn BDictAccess<B::BKey, B>) -> Result<&dyn BListAccess<B>, ParseError>
where
    B: BRefAccess<BType = B> + PartialEq,
{
    CONVERT.lookup_and_convert_list(info_dict, FILES_KEY)
}

// ----------------------------------------------------------------------------//

/// Parses the file dictionary from the file bencode.
#[allow(clippy::module_name_repetitions)]
pub fn parse_file_dict<B>(file_bencode: &B) -> Result<&dyn BDictAccess<B::BKey, B::BType>, ParseError>
where
    B: BRefAccess,
{
    CONVERT.convert_dict(file_bencode, FILES_KEY)
}

/// Parses the length from the info or file dictionary.
#[allow(clippy::module_name_repetitions)]
pub fn parse_length<B>(info_or_file_dict: &dyn BDictAccess<B::BKey, B>) -> Result<u64, ParseError>
where
    B: BRefAccess,
{
    CONVERT
        .lookup_and_convert_int(info_or_file_dict, LENGTH_KEY)
        .map(|len| len.try_into().unwrap())
}

/// Parses the md5sum from the info or file dictionary.
#[allow(clippy::module_name_repetitions)]
pub fn parse_md5sum<'a, B>(info_or_file_dict: &'a dyn BDictAccess<B::BKey, B>) -> Option<&'a [u8]>
where
    B: BRefAccess + 'a,
{
    CONVERT.lookup_and_convert_bytes(info_or_file_dict, MD5SUM_KEY).ok()
}

/// Parses the path list from the file dictionary.
#[allow(clippy::module_name_repetitions)]
pub fn parse_path_list<B>(file_dict: &dyn BDictAccess<B::BKey, B>) -> Result<&dyn BListAccess<B>, ParseError>
where
    B: BRefAccess<BType = B>,
{
    CONVERT.lookup_and_convert_list(file_dict, PATH_KEY)
}

/// Parses the path string from the path bencode.
#[allow(clippy::module_name_repetitions)]
pub fn parse_path_str<B>(path_bencode: &B) -> Result<&str, ParseError>
where
    B: BRefAccess,
{
    CONVERT.convert_str(path_bencode, PATH_KEY)
}
