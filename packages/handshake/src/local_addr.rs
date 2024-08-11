use std::net::SocketAddr;

/// Trait for getting the local address.

pub trait LocalAddr {
    /// Get the local address.
    ///
    /// # Errors
    ///
    /// It would return an IO Error if unable to obtain the local address.
    fn local_addr(&self) -> std::io::Result<SocketAddr>;
}
