//! Module for uber error types.

use error_chain::error_chain;

use crate::discovery::error::{DiscoveryError, DiscoveryErrorKind};

error_chain! {
    types {
        UberError, UberErrorKind, UberResultExt;
    }

    links {
        Discovery(DiscoveryError, DiscoveryErrorKind);
    }
}
