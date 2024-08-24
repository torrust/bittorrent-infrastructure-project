use thiserror::Error;

use crate::error::ErrorResponse;

/// Result type for a `ClientRequest`.
pub type ClientResult<T> = Result<T, ClientError>;

/// Errors occurring as the result of a `ClientRequest`.
#[allow(clippy::module_name_repetitions)]
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ClientError {
    #[error("Request timeout reached")]
    MaxTimeout,

    #[error("Request length exceeded the packet length")]
    MaxLength,

    #[error("Client shut down the request client")]
    ClientShutdown,

    #[error("Server sent us an invalid message")]
    ServerError,

    #[error("Requested to send from IPv4 to IPv6 or vice versa")]
    IPVersionMismatch,

    #[error("Server returned an error message : {0}")]
    ServerMessage(#[from] ErrorResponse<'static>),
}
