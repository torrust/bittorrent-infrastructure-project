use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use futures::channel::mpsc;
use futures::SinkExt as _;
use tokio::net::UdpSocket;
use tokio::task::JoinSet;
use util::bt::InfoHash;
use util::net;

use crate::handshaker_trait::HandshakerTrait;
use crate::router::Router;
use crate::worker::{self, DhtEvent, OneshotTask, ShutdownCause};

/// Maintains a Distributed Hash (Routing) Table.
pub struct MainlineDht {
    main_task_sender: mpsc::Sender<OneshotTask>,
    _tasks: JoinSet<()>,
}

impl MainlineDht {
    /// Start the `MainlineDht` with the given `DhtBuilder` and Handshaker.
    async fn with_builder<H>(builder: DhtBuilder, handshaker: H) -> std::io::Result<MainlineDht>
    where
        H: HandshakerTrait + 'static,
    {
        let send_sock = Arc::new(UdpSocket::bind(builder.src_addr).await?);
        let recv_sock = send_sock.clone();

        let kill_sock = send_sock.clone();
        let kill_addr = send_sock.local_addr()?;

        let (main_task_sender, tasks) = worker::start_mainline_dht(
            &send_sock,
            recv_sock,
            builder.read_only,
            builder.ext_addr,
            handshaker,
            kill_sock,
            kill_addr,
        );

        let nodes: Vec<SocketAddr> = builder.nodes.into_iter().collect();
        let routers: Vec<Router> = builder.routers.into_iter().collect();

        if main_task_sender
            .clone()
            .send(OneshotTask::StartBootstrap(routers, nodes))
            .await
            .is_err()
        {
            tracing::warn!("bip_dt: MainlineDht failed to send a start bootstrap message...");
        }

        Ok(MainlineDht {
            main_task_sender,
            _tasks: tasks,
        })
    }

    /// Perform a search for the given `InfoHash` with an optional announce on the closest nodes.
    ///
    ///
    /// Announcing will place your contact information in the DHT so others performing lookups
    /// for the `InfoHash` will be able to find your contact information and initiate a handshake.
    ///
    /// If the initial bootstrap has not finished, the search will be queued and executed once
    /// the bootstrap has completed.
    pub async fn search(&self, hash: InfoHash, announce: bool) {
        if self
            .main_task_sender
            .clone()
            .send(OneshotTask::StartLookup(hash, announce))
            .await
            .is_err()
        {
            tracing::warn!("bip_dht: MainlineDht failed to send a start lookup message...");
        }
    }

    /// An event Receiver which will receive events occurring within the DHT.
    ///
    /// It is important to at least monitor the DHT for shutdown events as any calls
    /// after that event occurs will not be processed but no indication will be given.
    #[must_use]
    pub async fn events(&self) -> mpsc::Receiver<DhtEvent> {
        let (send, recv) = mpsc::channel(1);

        if let Err(e) = self.main_task_sender.clone().send(OneshotTask::RegisterSender(send)).await {
            tracing::warn!("bip_dht: MainlineDht failed to send a register sender message..., {e}");
            // TODO: Should we push a Shutdown event through the sender here? We would need
            // to know the cause or create a new cause for this specific scenario since the
            // client could have been lazy and wasn't monitoring this until after it shutdown!
        }

        recv
    }
}

impl Drop for MainlineDht {
    fn drop(&mut self) {
        if self
            .main_task_sender
            .clone()
            .try_send(OneshotTask::Shutdown(ShutdownCause::ClientInitiated))
            .is_err()
        {
            tracing::warn!(
                "bip_dht: MainlineDht failed to send a shutdown message (may have already been \
                   shutdown)..."
            );
        }
    }
}

// ----------------------------------------------------------------------------//

/// Stores information for initializing a DHT.
#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Debug)]
pub struct DhtBuilder {
    nodes: HashSet<SocketAddr>,
    routers: HashSet<Router>,
    read_only: bool,
    src_addr: SocketAddr,
    ext_addr: Option<SocketAddr>,
}

impl DhtBuilder {
    /// Create a new `DhtBuilder`.
    ///
    /// This should not be used directly, force the user to supply builder with
    /// some initial bootstrap method.
    fn new() -> DhtBuilder {
        DhtBuilder {
            nodes: HashSet::new(),
            routers: HashSet::new(),
            read_only: true,
            src_addr: net::default_route_v4(),
            ext_addr: None,
        }
    }

    /// Creates a `DhtBuilder` with an initial node for our routing table.
    #[must_use]
    pub fn with_node(node_addr: SocketAddr) -> DhtBuilder {
        let dht = DhtBuilder::new();

        dht.add_node(node_addr)
    }

    /// Creates a `DhtBuilder` with an initial router which will let us gather nodes
    /// if our routing table is ever empty.
    ///
    /// Difference between a node and a router is that a router is never put in
    /// our routing table.
    #[must_use]
    pub fn with_router(router: Router) -> DhtBuilder {
        let dht = DhtBuilder::new();

        dht.add_router(router)
    }

    /// Add nodes which will be distributed within our routing table.
    #[must_use]
    pub fn add_node(mut self, node_addr: SocketAddr) -> DhtBuilder {
        self.nodes.insert(node_addr);

        self
    }

    /// Add a router which will let us gather nodes if our routing table is ever empty.
    ///
    /// See `DhtBuilder::with_router` for difference between a router and a node.
    #[must_use]
    pub fn add_router(mut self, router: Router) -> DhtBuilder {
        self.routers.insert(router);

        self
    }

    /// Set the read only flag when communicating with other nodes. Indicates
    /// that remote nodes should not add us to their routing table.
    ///
    /// Used when we are behind a restrictive NAT and/or we want to decrease
    /// incoming network traffic. Defaults value is true.
    #[must_use]
    pub fn set_read_only(mut self, read_only: bool) -> DhtBuilder {
        self.read_only = read_only;

        self
    }

    /// Provide the DHT with our external address. If this is not supplied we will
    /// have to deduce this information from remote nodes.
    ///
    /// Purpose of the external address is to generate a `NodeId` that conforms to
    /// BEP 42 so that nodes can safely store information on our node.
    #[must_use]
    pub fn set_external_addr(mut self, addr: SocketAddr) -> DhtBuilder {
        self.ext_addr = Some(addr);

        self
    }

    /// Provide the DHT with the source address.
    ///
    /// If this is not supplied we will use the OS default route.
    #[must_use]
    pub fn set_source_addr(mut self, addr: SocketAddr) -> DhtBuilder {
        self.src_addr = addr;

        self
    }

    /// Start a mainline DHT with the current configuration.
    ///
    /// # Errors
    ///
    /// It would return error if unable to build from the handshaker.
    pub async fn start_mainline<H>(self, handshaker: H) -> std::io::Result<MainlineDht>
    where
        H: HandshakerTrait + 'static,
    {
        MainlineDht::with_builder(self, handshaker).await
    }
}
