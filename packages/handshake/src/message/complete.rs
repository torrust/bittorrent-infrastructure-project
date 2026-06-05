use std::net::SocketAddr;

use torrust_util::bt::{InfoHash, PeerId};

use crate::message::extensions::Extensions;
use crate::message::protocol::Protocol;

/// Message containing completed handshaking information.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct CompleteMessage<S> {
    prot: Protocol,
    ext: Extensions,
    hash: InfoHash,
    pid: PeerId,
    addr: SocketAddr,
    sock: S,
}

impl<S> CompleteMessage<S> {
    /// Create a new `CompleteMessage` over the given socket S.
    pub const fn new(prot: Protocol, ext: Extensions, hash: InfoHash, pid: PeerId, addr: SocketAddr, sock: S) -> Self {
        Self {
            prot,
            ext,
            hash,
            pid,
            addr,
            sock,
        }
    }

    /// Protocol that this peer is operating over.
    pub const fn protocol(&self) -> &Protocol {
        &self.prot
    }

    /// Extensions that both you and the peer support.
    pub const fn extensions(&self) -> &Extensions {
        &self.ext
    }

    /// Hash that the peer is interested in.
    pub const fn hash(&self) -> &InfoHash {
        &self.hash
    }

    /// Id that the peer has given itself.
    pub const fn peer_id(&self) -> &PeerId {
        &self.pid
    }

    /// Address the peer is connected to us on.
    pub const fn address(&self) -> &SocketAddr {
        &self.addr
    }

    /// Socket of some type S, that we use to communicate with the peer.
    pub const fn socket(&self) -> &S {
        &self.sock
    }

    /// Break the `CompleteMessage` into its parts.
    pub fn into_parts(self) -> (Protocol, Extensions, InfoHash, PeerId, SocketAddr, S) {
        (self.prot, self.ext, self.hash, self.pid, self.addr, self.sock)
    }
}
