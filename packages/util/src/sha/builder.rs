use crypto::digest::Digest;
use crypto::sha1::Sha1;

use crate::sha::{self, ShaHash};

/// Building `ShaHash` objects by adding byte slices to the hash.
#[allow(clippy::module_name_repetitions)]
#[derive(Clone)]
pub struct ShaHashBuilder {
    sha: Sha1,
}

impl Default for ShaHashBuilder {
    fn default() -> Self {
        Self { sha: Sha1::new() }
    }
}

impl ShaHashBuilder {
    /// Create a new `ShaHashBuilder`.
    #[must_use]
    pub fn new() -> ShaHashBuilder {
        ShaHashBuilder::default()
    }

    /// Add bytes to the `ShaHashBuilder`.
    #[must_use]
    pub fn add_bytes(mut self, bytes: &[u8]) -> ShaHashBuilder {
        self.sha.input(bytes);

        self
    }

    /// Build the `ShaHash` from the `ShaHashBuilder`.
    #[must_use]
    pub fn build(&self) -> ShaHash {
        let mut buffer = [0u8; sha::SHA_HASH_LEN];

        self.sha.clone().result(&mut buffer);

        buffer.into()
    }
}
