//! `BitTorrent` [`InfoHash`] v1 type for Rust crates.

#![allow(clippy::module_name_repetitions)]

mod info_hash;

pub use self::info_hash::{ConversionError, INFO_HASH_BYTES_LEN, InfoHash, fixture};
