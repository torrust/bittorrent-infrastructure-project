use std::net::SocketAddr;
use std::sync::Arc;

use futures::channel::mpsc;
use tokio::net::UdpSocket;
use tokio::task::JoinSet;
use util::bt::InfoHash;

use crate::handshaker_trait::HandshakerTrait;
use crate::router::Router;
use crate::routing::table::{self, RoutingTable};
use crate::transaction::TransactionID;

pub mod bootstrap;
pub mod handler;
pub mod lookup;
pub mod messenger;
pub mod refresh;

/// Task that our DHT will execute immediately.
#[derive(Clone)]
pub enum OneshotTask {
    /// Process an incoming message from a remote node.
    Incoming(Vec<u8>, SocketAddr),
    /// Register a sender to send `DhtEvents` to.
    RegisterSender(mpsc::Sender<DhtEvent>),
    /// Load a new bootstrap operation into worker storage.
    StartBootstrap(Vec<Router>, Vec<SocketAddr>),
    /// Start a lookup for the given `InfoHash`.
    StartLookup(InfoHash, bool),
    /// Gracefully shutdown the DHT and associated workers.
    Shutdown(ShutdownCause),
}

/// Task that our DHT will execute some time later.
#[derive(Copy, Clone, Debug)]
pub enum ScheduledTaskCheck {
    /// Check the progress of the bucket refresh.
    TableRefresh(TransactionID),
    /// Check the progress of the current bootstrap.
    BootstrapTimeout(TransactionID),
    /// Check the progress of a current lookup.
    LookupTimeout(TransactionID),
    /// Check the progress of the lookup endgame.
    #[allow(dead_code)]
    LookupEndGame(TransactionID),
}

/// Event that occurred within the DHT which clients may be interested in.
#[derive(Copy, Clone, Debug)]
pub enum DhtEvent {
    /// DHT completed the bootstrap.
    BootstrapCompleted,
    /// Lookup operation for the given `InfoHash` completed.
    LookupCompleted(InfoHash),
    /// DHT is shutting down for some reason.
    ShuttingDown(ShutdownCause),
}

/// Event that occurred within the DHT which caused it to shutdown.
#[derive(Copy, Clone, Debug)]
pub enum ShutdownCause {
    /// DHT failed to bootstrap more than once.
    BootstrapFailed,
    /// Client controlling the DHT intentionally shut it down.
    ClientInitiated,
    /// Cause of shutdown is not specified.
    Unspecified,
}

/// Spawns the necessary workers that make up our local DHT node and connects them via channels
/// so that they can send and receive DHT messages.
pub fn start_mainline_dht<H>(
    send_socket: &Arc<UdpSocket>,
    recv_socket: Arc<UdpSocket>,
    read_only: bool,
    _: Option<SocketAddr>,
    handshaker: H,
    kill_sock: Arc<UdpSocket>,
    kill_addr: SocketAddr,
) -> (mpsc::Sender<OneshotTask>, JoinSet<()>)
where
    H: HandshakerTrait + 'static,
{
    let outgoing = messenger::create_outgoing_messenger(send_socket);

    // TODO: Utilize the security extension.
    let routing_table = RoutingTable::new(table::random_node_id());
    let message_sender = handler::create_dht_handler(routing_table, outgoing, read_only, handshaker, kill_sock, kill_addr);

    messenger::create_incoming_messenger(recv_socket, message_sender.0.clone());

    message_sender
}
