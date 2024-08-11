//! Module for uber error types.

use thiserror::Error;

use crate::discovery::error::DiscoveryError;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
}
