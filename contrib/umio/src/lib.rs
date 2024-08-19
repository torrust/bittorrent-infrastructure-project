//! Message Based Readiness API
//!
//! This library is a thin wrapper around mio for clients who wish to
//! use a single udp socket in conjunction with message passing and
//! timeouts.

mod buffer;
mod dispatcher;
mod eloop;
mod provider;

/// Exports of bare mio types.
pub mod external;

pub use dispatcher::Dispatcher;
pub use eloop::{ELoop, ELoopBuilder};
pub use provider::Provider;
