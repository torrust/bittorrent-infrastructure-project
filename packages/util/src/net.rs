use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4};

/// Abstraction of some ip address.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum IpAddr {
    V4(Ipv4Addr),
    V6(Ipv6Addr),
}

impl IpAddr {
    /// Create a new `IpAddr` from the given `SocketAddr`.
    #[must_use]
    pub const fn from_socket_addr(sock_addr: SocketAddr) -> Self {
        match sock_addr {
            SocketAddr::V4(v4_sock_addr) => Self::V4(*v4_sock_addr.ip()),
            SocketAddr::V6(v6_sock_addr) => Self::V6(*v6_sock_addr.ip()),
        }
    }
}

/// Get the default route ipv4 socket.
#[must_use]
pub const fn default_route_v4() -> SocketAddr {
    let v4_sock = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0);

    SocketAddr::V4(v4_sock)
}
