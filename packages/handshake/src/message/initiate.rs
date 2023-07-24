use std::net::SocketAddr;

use util::bt::InfoHash;

use crate::message::protocol::Protocol;

/// Message used to initiate a handshake with the `Handshaker`.
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct InitiateMessage {
    prot: Protocol,
    hash: InfoHash,
    addr: SocketAddr,
}

impl InitiateMessage {
    /// Create a new `InitiateMessage`.
    #[must_use]
    pub fn new(prot: Protocol, hash: InfoHash, addr: SocketAddr) -> InitiateMessage {
        InitiateMessage { prot, hash, addr }
    }

    /// Protocol that we want to connect to the peer with.
    #[must_use]
    pub fn protocol(&self) -> &Protocol {
        &self.prot
    }

    /// Hash that we are interested in from the peer.
    #[must_use]
    pub fn hash(&self) -> &InfoHash {
        &self.hash
    }

    /// Address that we should connect to for the peer.
    #[must_use]
    pub fn address(&self) -> &SocketAddr {
        &self.addr
    }

    /// Break the `InitiateMessage` up into its parts.
    #[must_use]
    pub fn into_parts(self) -> (Protocol, InfoHash, SocketAddr) {
        (self.prot, self.hash, self.addr)
    }
}
