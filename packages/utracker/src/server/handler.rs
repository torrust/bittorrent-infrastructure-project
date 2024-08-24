use std::net::SocketAddr;

use crate::announce::{AnnounceRequest, AnnounceResponse};
use crate::scrape::{ScrapeRequest, ScrapeResponse};

/// Result type for a `ServerHandler`.
///
/// Either the response T or an error message.
pub type ServerResult<'a, T> = Result<T, &'a str>;

/// Trait for providing a `TrackerServer` with methods to service `TrackerRequests`.
#[allow(clippy::module_name_repetitions)]

pub trait ServerHandler: Send {
    /// Service a connection id request from the given address.
    fn connect(&mut self, addr: SocketAddr) -> Option<ServerResult<'_, u64>>;

    /// Service an announce request with the given connect id.
    fn announce(
        &mut self,
        addr: SocketAddr,
        id: u64,
        req: &AnnounceRequest<'_>,
    ) -> Option<ServerResult<'_, AnnounceResponse<'_>>>;

    /// Service a scrape request with the given connect id.
    fn scrape(&mut self, addr: SocketAddr, id: u64, req: &ScrapeRequest<'_>) -> Option<ServerResult<'_, ScrapeResponse<'_>>>;
}
