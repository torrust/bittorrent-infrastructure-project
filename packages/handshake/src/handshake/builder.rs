use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use rand::Rng as _;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::task::JoinSet;
use util::bt::PeerId;
use util::convert;

use super::Handshaker;
use crate::{Extensions, HandshakerConfig, Transport};

/// Build configuration for `Handshaker` object creation.
#[allow(clippy::module_name_repetitions)]
#[derive(Copy, Clone)]
pub struct HandshakerBuilder {
    pub(super) bind: SocketAddr,
    pub(super) port: u16,
    pub(super) pid: PeerId,
    pub(super) ext: Extensions,
    pub(super) config: HandshakerConfig,
}

impl Default for HandshakerBuilder {
    fn default() -> Self {
        let default_v4_addr = Ipv4Addr::new(0, 0, 0, 0);
        let default_v4_port = 0;

        let bind = SocketAddr::V4(SocketAddrV4::new(default_v4_addr, default_v4_port));

        let seed = rand::thread_rng().gen();
        let pid = PeerId::from_bytes(&convert::four_bytes_to_array(seed));

        Self {
            bind,
            port: Default::default(),
            pid,
            ext: Extensions::default(),
            config: HandshakerConfig::default(),
        }
    }
}

impl HandshakerBuilder {
    /// Create a new `HandshakerBuilder`.
    #[must_use]
    pub fn new() -> HandshakerBuilder {
        Self::default()
    }

    /// Address that the host will listen on.
    ///
    /// Defaults to `IN_ADDR_ANY` using port 0 (any free port).
    pub fn with_bind_addr(&mut self, addr: SocketAddr) -> &mut HandshakerBuilder {
        self.bind = addr;

        self
    }

    /// Port that external peers should connect on.
    ///
    /// Defaults to the port that is being listened on (will only work if the
    /// host is not natted).
    pub fn with_open_port(&mut self, port: u16) -> &mut HandshakerBuilder {
        self.port = port;

        self
    }

    /// Peer id that will be advertised when handshaking with other peers.
    ///
    /// Defaults to a random SHA-1 hash; official clients should use an encoding scheme.
    ///
    /// See [BEP 0020](http://www.bittorrent.org/beps/bep_0020.html).
    pub fn with_peer_id(&mut self, peer_id: PeerId) -> &mut HandshakerBuilder {
        self.pid = peer_id;

        self
    }

    /// Extensions supported by our client, advertised to the peer when handshaking.
    pub fn with_extensions(&mut self, ext: Extensions) -> &mut HandshakerBuilder {
        self.ext = ext;

        self
    }

    /// Configuration that will be used to alter the internal behavior of handshaking.
    ///
    /// This will typically not need to be set unless you know what you are doing.
    pub fn with_config(&mut self, config: HandshakerConfig) -> &mut HandshakerBuilder {
        self.config = config;

        self
    }

    /// Build a `Handshaker` over the given `Transport` with a `Remote` instance.
    ///
    /// # Errors
    ///
    /// Returns a IO error if unable to build.
    pub async fn build<T>(&self, transport: T) -> std::io::Result<(Handshaker<T::Socket>, JoinSet<()>)>
    where
        T: Transport + Send + Sync + 'static,
        <T as Transport>::Socket: AsyncWrite + AsyncRead + std::fmt::Debug + Send + Sync,
        <T as Transport>::Listener: Send,
    {
        Handshaker::with_builder(self, transport).await
    }
}
