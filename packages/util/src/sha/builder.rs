use sha1::{Digest, Sha1};

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
    pub fn new() -> Self {
        Self::default()
    }

    /// Add bytes to the `ShaHashBuilder`.
    #[must_use]
    pub fn add_bytes(mut self, bytes: &[u8]) -> Self {
        self.sha.update(bytes);

        self
    }

    /// Build the `ShaHash` from the `ShaHashBuilder`.
    #[must_use]
    pub fn build(&self) -> ShaHash {
        let mut buffer = [0u8; sha::SHA_HASH_LEN];

        let digest = self.sha.clone().finalize();
        buffer.copy_from_slice(&digest);

        buffer.into()
    }
}
