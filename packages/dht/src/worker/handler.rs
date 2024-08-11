use std::collections::HashMap;
use std::convert::AsRef;
use std::net::{SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use bencode::{ben_bytes, BDecodeOpt, BencodeMut, BencodeRef};
use futures::channel::mpsc;
use futures::future::BoxFuture;
use futures::{FutureExt, SinkExt, StreamExt as _};
use tokio::net::UdpSocket;
use tokio::task::JoinSet;
use util::bt::InfoHash;
use util::convert;
use util::net::IpAddr;

use crate::handshaker_trait::HandshakerTrait;
use crate::message::announce_peer::{AnnouncePeerResponse, ConnectPort};
use crate::message::compact_info::{CompactNodeInfo, CompactValueInfo};
use crate::message::error::{ErrorCode, ErrorMessage};
use crate::message::find_node::FindNodeResponse;
use crate::message::get_peers::{CompactInfoType, GetPeersResponse};
use crate::message::ping::PingResponse;
use crate::message::request::RequestType;
use crate::message::response::{ExpectedResponse, ResponseType};
use crate::message::MessageType;
use crate::router::Router;
use crate::routing::node::{Node, NodeStatus};
use crate::routing::table::{BucketContents, RoutingTable};
use crate::storage::AnnounceStorage;
use crate::token::{Token, TokenStore};
use crate::transaction::{AIDGenerator, ActionID, TransactionID};
use crate::worker::bootstrap::{BootstrapStatus, TableBootstrap};
use crate::worker::lookup::{LookupStatus, TableLookup};
use crate::worker::refresh::{RefreshStatus, TableRefresh};
use crate::worker::{DhtEvent, OneshotTask, ScheduledTaskCheck, ShutdownCause};

const MAX_BOOTSTRAP_ATTEMPTS: usize = 3;
const BOOTSTRAP_GOOD_NODE_THRESHOLD: usize = 10;

enum Task {
    Main(OneshotTask),
    Scheduled(ScheduledTaskCheck),
}

/// Spawns a DHT handler that maintains our routing table and executes our actions on the DHT.
#[allow(clippy::module_name_repetitions)]
pub fn create_dht_handler<H>(
    table: RoutingTable,
    out: mpsc::Sender<(Vec<u8>, SocketAddr)>,
    read_only: bool,
    handshaker: H,
    kill_sock: Arc<UdpSocket>,
    kill_addr: SocketAddr,
) -> (mpsc::Sender<OneshotTask>, JoinSet<()>)
where
    H: HandshakerTrait + 'static,
{
    let (main_task_sender, main_task_receiver) = mpsc::channel(100);
    let (scheduled_task_sender, scheduled_task_receiver) = mpsc::channel(100);

    let main_task_receiver = main_task_receiver.map(Task::Main);
    let scheduled_task_receiver = scheduled_task_receiver.map(Task::Scheduled);

    let mut tasks_receiver = futures::stream::select(main_task_receiver, scheduled_task_receiver);

    let handler = DhtHandler::new(
        table,
        out,
        main_task_sender.clone(),
        scheduled_task_sender,
        read_only,
        handshaker,
    );

    let mut tasks = JoinSet::new();

    tasks.spawn(async move {
        while let Some(task) = tasks_receiver.next().await {
            match task {
                Task::Main(main_task) => handler.handle_task(main_task).await,
                Task::Scheduled(scheduled_task) => handler.handle_scheduled_task(scheduled_task).await,
            }
        }

        // When event loop stops, we need to "wake" the incoming messenger with a socket message,
        // when it processes the message and tries to pass it to us, it will see that our channel
        // is closed and know that it should shut down. The outgoing messenger will shut itself down.
        if kill_sock.send_to(&b"0"[..], kill_addr).await.is_err() {
            tracing::error!("bip_dht: Failed to send a wake up message to the incoming channel...");
        }

        tracing::info!("bip_dht: DhtHandler gracefully shut down, exiting thread...");
    });

    (main_task_sender, tasks)
}

// ----------------------------------------------------------------------------//

/// Actions that we can perform on our `RoutingTable`.
#[derive(Clone)]
enum TableAction {
    /// Lookup action.
    Lookup(Arc<TableLookup>),
    /// Refresh action.
    Refresh(Arc<TableRefresh>),
    /// Bootstrap action.
    ///
    /// Includes number of bootstrap attempts.
    Bootstrap(Arc<TableBootstrap>, Arc<AtomicUsize>),
}

/// Actions that we want to perform on our `RoutingTable` after bootstrapping finishes.
enum PostBootstrapAction {
    /// Future lookup action.
    Lookup(InfoHash, bool),
    /// Future refresh action.
    Refresh(Box<TableRefresh>, TransactionID),
}

#[allow(clippy::module_name_repetitions)]
pub struct DhtHandler<H> {
    handshaker: futures::lock::Mutex<H>,
    routing_table: Arc<RwLock<RoutingTable>>,
    table_actions: Mutex<HashMap<ActionID, TableAction>>,

    out_channel: mpsc::Sender<(Vec<u8>, SocketAddr)>,
    main_task_sender: mpsc::Sender<OneshotTask>,
    scheduled_task_sender: mpsc::Sender<ScheduledTaskCheck>,

    read_only: bool,
    bootstrapping: AtomicBool,

    token_store: Mutex<TokenStore>,
    aid_generator: Mutex<AIDGenerator>,
    active_stores: Mutex<AnnounceStorage>,

    // If future actions is not empty, that means we are still bootstrapping
    // since we will always spin up a table refresh action after bootstrapping.
    future_actions: Mutex<Vec<PostBootstrapAction>>,
    event_notifiers: Mutex<Vec<mpsc::Sender<DhtEvent>>>,
}

impl<H> DhtHandler<H>
where
    H: HandshakerTrait + 'static,
{
    fn new(
        table: RoutingTable,
        out: mpsc::Sender<(Vec<u8>, SocketAddr)>,
        main_task_sender: mpsc::Sender<OneshotTask>,
        scheduled_task_sender: mpsc::Sender<ScheduledTaskCheck>,
        read_only: bool,
        handshaker: H,
    ) -> DhtHandler<H> {
        let mut aid_generator = AIDGenerator::new();

        // Insert the refresh task to execute after the bootstrap
        let mut mid_generator = aid_generator.generate();
        let refresh_trans_id = mid_generator.generate();
        let table_refresh = Box::new(TableRefresh::new(mid_generator));
        let future_actions = vec![PostBootstrapAction::Refresh(table_refresh, refresh_trans_id)];

        DhtHandler {
            read_only,
            handshaker: futures::lock::Mutex::new(handshaker),
            out_channel: out,
            token_store: Mutex::new(TokenStore::new()),
            aid_generator: Mutex::new(aid_generator),
            bootstrapping: AtomicBool::default(),
            routing_table: Arc::new(RwLock::new(table)),
            active_stores: Mutex::new(AnnounceStorage::new()),
            future_actions: Mutex::new(future_actions),
            event_notifiers: Mutex::default(),
            table_actions: Mutex::new(HashMap::new()),
            main_task_sender,
            scheduled_task_sender,
        }
    }

    async fn handle_task(&self, task: OneshotTask) {
        match task {
            OneshotTask::Incoming(buffer, addr) => {
                self.handle_incoming(&buffer[..], addr).await;
            }
            OneshotTask::RegisterSender(send) => {
                self.handle_register_sender(send);
            }
            OneshotTask::StartBootstrap(routers, nodes) => {
                self.handle_start_bootstrap(routers, nodes).await;
            }
            OneshotTask::StartLookup(info_hash, should_announce) => {
                self.handle_start_lookup(info_hash, should_announce).await;
            }
            OneshotTask::Shutdown(cause) => {
                self.handle_shutdown(cause);
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn handle_incoming(&self, buffer: &[u8], addr: SocketAddr) {
        // Parse the buffer as a bencoded message
        let Ok(bencode) = BencodeRef::decode(buffer, BDecodeOpt::default()) else {
            tracing::warn!("bip_dht: Received invalid bencode data...");
            return;
        };

        // Parse the bencode as a message
        // Check to make sure we issued the transaction id (or that it is still valid)
        let message = MessageType::<BencodeRef<'_>>::new(&bencode, |trans| {
            // Check if we can interpret the response transaction id as one of ours.
            let Some(trans_id) = TransactionID::from_bytes(trans) else {
                return ExpectedResponse::None;
            };

            // Match the response action id with our current actions
            let Some(table_action) = self.table_actions.lock().unwrap().get(&trans_id.action_id()).cloned() else {
                return ExpectedResponse::None;
            };

            match table_action {
                TableAction::Lookup(_) => ExpectedResponse::GetPeers,
                TableAction::Refresh(_) | TableAction::Bootstrap(_, _) => ExpectedResponse::FindNode,
            }
        });

        // Do not process requests if we are read only
        // TODO: Add read only flags to messages we send it we are read only!
        // Also, check for read only flags on responses we get before adding nodes
        // to our RoutingTable.
        if self.read_only {
            if let Ok(MessageType::Request(_)) = message {
                return;
            }
        }

        // Process the given message
        match message {
            Ok(MessageType::Request(RequestType::Ping(p))) => {
                tracing::info!("bip_dht: Received a PingRequest...");
                let node = Node::as_good(p.node_id(), addr);

                let ping_rsp = {
                    let routing_table = self.routing_table.read().unwrap();

                    // Node requested from us, mark it in the routing table
                    if let Some(n) = routing_table.find_node(&node) {
                        n.remote_request();
                    }

                    PingResponse::new(p.transaction_id(), routing_table.node_id())
                };

                let ping_msg = ping_rsp.encode();

                let mut out = self.out_channel.clone();

                let sent = out.send((ping_msg, addr)).await;

                if sent.is_err() {
                    tracing::error!("bip_dht: Failed to send a ping response on the out channel...");
                    self.handle_shutdown(ShutdownCause::Unspecified);
                }
            }
            Ok(MessageType::Request(RequestType::FindNode(f))) => {
                tracing::info!("bip_dht: Received a FindNodeRequest...");
                let node = Node::as_good(f.node_id(), addr);

                let find_node_msg = {
                    let routing_table = self.routing_table.read().unwrap();

                    // Node requested from us, mark it in the routing table
                    if let Some(n) = routing_table.find_node(&node) {
                        n.remote_request();
                    }

                    // Grab the closest nodes
                    let mut closest_nodes_bytes = Vec::with_capacity(26 * 8);
                    for node in routing_table.closest_nodes(f.target_id()).take(8) {
                        closest_nodes_bytes.extend_from_slice(&node.encode());
                    }

                    let find_node_rsp =
                        FindNodeResponse::new(f.transaction_id(), routing_table.node_id(), &closest_nodes_bytes).unwrap();
                    find_node_rsp.encode()
                };

                if self.out_channel.clone().send((find_node_msg, addr)).await.is_err() {
                    tracing::error!("bip_dht: Failed to send a find node response on the out channel...");
                    self.handle_shutdown(ShutdownCause::Unspecified);
                }
            }
            Ok(MessageType::Request(RequestType::GetPeers(g))) => {
                tracing::info!("bip_dht: Received a GetPeersRequest...");
                let node = Node::as_good(g.node_id(), addr);

                let get_peers_msg = {
                    let routing_table = self.routing_table.read().unwrap();

                    // Node requested from us, mark it in the routing table
                    if let Some(n) = routing_table.find_node(&node) {
                        n.remote_request();
                    }

                    // TODO: Move socket address serialization code into bip_util
                    // TODO: Check what the maximum number of values we can give without overflowing a udp packet
                    // Also, if we aren't going to give all of the contacts, we may want to shuffle which ones we give
                    let mut contact_info_bytes = Vec::with_capacity(6 * 20);
                    self.active_stores.lock().unwrap().find_items(&g.info_hash(), |addr| {
                        let mut bytes = [0u8; 6];
                        let port = addr.port();

                        match addr {
                            SocketAddr::V4(v4_addr) => {
                                for (src, dst) in convert::ipv4_to_bytes_be(*v4_addr.ip()).iter().zip(bytes.iter_mut()) {
                                    *dst = *src;
                                }
                            }
                            SocketAddr::V6(_) => {
                                tracing::error!("AnnounceStorage contained an IPv6 Address...");
                                return;
                            }
                        };

                        bytes[4] = (port >> 8) as u8;
                        bytes[5] = (port & 0x00FF) as u8;

                        contact_info_bytes.extend_from_slice(&bytes);
                    });
                    // Grab the bencoded list (ugh, we really have to do this, better apis I say!!!)
                    let mut contact_info_bencode = Vec::with_capacity(contact_info_bytes.len() / 6);
                    for chunk_index in 0..(contact_info_bytes.len() / 6) {
                        let (start, end) = (chunk_index * 6, chunk_index * 6 + 6);

                        contact_info_bencode.push(ben_bytes!(&contact_info_bytes[start..end]));
                    }

                    // Grab the closest nodes
                    let mut closest_nodes_bytes = Vec::with_capacity(26 * 8);
                    for node in routing_table.closest_nodes(g.info_hash()).take(8) {
                        closest_nodes_bytes.extend_from_slice(&node.encode());
                    }

                    // Wrap up the nodes/values we are going to be giving them
                    let token = self.token_store.lock().unwrap().checkout(IpAddr::from_socket_addr(addr));
                    let compact_info_type = if contact_info_bencode.is_empty() {
                        CompactInfoType::Nodes(CompactNodeInfo::new(&closest_nodes_bytes).unwrap())
                    } else {
                        CompactInfoType::<BencodeMut<'_>>::Both(
                            CompactNodeInfo::new(&closest_nodes_bytes).unwrap(),
                            CompactValueInfo::new(&contact_info_bencode).unwrap(),
                        )
                    };

                    let get_peers_rsp = GetPeersResponse::<BencodeMut<'_>>::new(
                        g.transaction_id(),
                        routing_table.node_id(),
                        Some(token.as_ref()),
                        compact_info_type,
                    );

                    get_peers_rsp.encode()
                };

                if self.out_channel.clone().send((get_peers_msg, addr)).await.is_err() {
                    tracing::error!("bip_dht: Failed to send a get peers response on the out channel...");
                    self.handle_shutdown(ShutdownCause::Unspecified);
                }
            }
            Ok(MessageType::Request(RequestType::AnnouncePeer(a))) => {
                tracing::info!("bip_dht: Received an AnnouncePeerRequest...");
                let node = Node::as_good(a.node_id(), addr);

                let response_msg = {
                    let routing_table = self.routing_table.read().unwrap();

                    // Node requested from us, mark it in the routing table
                    if let Some(n) = routing_table.find_node(&node) {
                        n.remote_request();
                    }

                    // Validate the token
                    let is_valid = match Token::new(a.token()) {
                        Ok(t) => self.token_store.lock().unwrap().checkin(IpAddr::from_socket_addr(addr), t),
                        Err(_) => false,
                    };

                    // Create a socket address based on the implied/explicit port number
                    let connect_addr = match a.connect_port() {
                        ConnectPort::Implied => addr,
                        ConnectPort::Explicit(port) => match addr {
                            SocketAddr::V4(v4_addr) => SocketAddr::V4(SocketAddrV4::new(*v4_addr.ip(), port)),
                            SocketAddr::V6(v6_addr) => {
                                SocketAddr::V6(SocketAddrV6::new(*v6_addr.ip(), port, v6_addr.flowinfo(), v6_addr.scope_id()))
                            }
                        },
                    };

                    // Resolve type of response we are going to send
                    if !is_valid {
                        // Node gave us an invalid token
                        tracing::warn!("bip_dht: Remote node sent us an invalid token for an AnnounceRequest...");
                        ErrorMessage::new(
                            a.transaction_id().to_vec(),
                            ErrorCode::ProtocolError,
                            "Received An Invalid Token".to_owned(),
                        )
                        .encode()
                    } else if self.active_stores.lock().unwrap().add_item(a.info_hash(), connect_addr) {
                        // Node successfully stored the value with us, send an announce response
                        AnnouncePeerResponse::new(a.transaction_id(), routing_table.node_id()).encode()
                    } else {
                        // Node unsuccessfully stored the value with us, send them an error message
                        // TODO: Spec doesn't actually say what error message to send, or even if we should send one...
                        tracing::warn!(
                            "bip_dht: AnnounceStorage failed to store contact information because it \
                           is full..."
                        );
                        ErrorMessage::new(
                            a.transaction_id().to_vec(),
                            ErrorCode::ServerError,
                            "Announce Storage Is Full".to_owned(),
                        )
                        .encode()
                    }
                };

                if self.out_channel.clone().send((response_msg, addr)).await.is_err() {
                    tracing::error!("bip_dht: Failed to send an announce peer response on the out channel...");
                    self.handle_shutdown(ShutdownCause::Unspecified);
                }
            }
            Ok(MessageType::Response(ResponseType::FindNode(f))) => {
                tracing::info!("bip_dht: Received a FindNodeResponse...");
                let trans_id = TransactionID::from_bytes(f.transaction_id()).unwrap();
                let node = Node::as_good(f.node_id(), addr);

                let opt_bootstrap = {
                    let mut routing_table = self.routing_table.write().unwrap();

                    // Add the payload nodes as questionable
                    for (id, v4_addr) in f.nodes() {
                        let sock_addr = SocketAddr::V4(v4_addr);

                        routing_table.add_node(&Node::as_questionable(id, sock_addr));
                    }

                    // Match the response action id with our current actions
                    let table_action = self.table_actions.lock().unwrap().get(&trans_id.action_id()).cloned();

                    match table_action {
                        Some(TableAction::Refresh(_)) => {
                            routing_table.add_node(&node);
                            None
                        }
                        Some(TableAction::Bootstrap(bootstrap, attempts)) => {
                            if !bootstrap.is_router(&node.addr()) {
                                routing_table.add_node(&node);
                            }
                            Some((bootstrap, attempts))
                        }
                        Some(TableAction::Lookup(_)) => {
                            tracing::error!("bip_dht: Resolved a FindNodeResponse ActionID to a TableLookup...");
                            None
                        }
                        None => {
                            tracing::error!(
                                "bip_dht: Resolved a TransactionID to a FindNodeResponse but no \
                            action found..."
                            );
                            None
                        }
                    }
                };

                let bootstrap_complete = {
                    if let Some((bootstrap, attempts)) = opt_bootstrap {
                        let response = bootstrap
                            .recv_response::<H>(
                                trans_id,
                                self.routing_table.clone(),
                                self.out_channel.clone(),
                                self.scheduled_task_sender.clone(),
                            )
                            .await;

                        match response {
                            BootstrapStatus::Idle => true,
                            BootstrapStatus::Bootstrapping => false,
                            BootstrapStatus::Failed => {
                                self.handle_shutdown(ShutdownCause::Unspecified);
                                false
                            }
                            BootstrapStatus::Completed => {
                                if should_rebootstrap(&self.routing_table.read().unwrap()) {
                                    attempt_rebootstrap(
                                        bootstrap.clone(),
                                        attempts.clone(),
                                        self.routing_table.clone(),
                                        self.out_channel.clone(),
                                        self.main_task_sender.clone(),
                                        self.scheduled_task_sender.clone(),
                                    )
                                    .await
                                        == Some(false)
                                } else {
                                    true
                                }
                            }
                        }
                    } else {
                        false
                    }
                };

                if bootstrap_complete {
                    self.broadcast_bootstrap_completed(trans_id.action_id()).await;
                }

                let routing_table = self.routing_table.read().unwrap();

                if tracing::enabled!(tracing::Level::INFO) {
                    let mut total = 0;

                    for (index, bucket) in routing_table.buckets().enumerate() {
                        let num_nodes = match bucket {
                            BucketContents::Empty => 0,
                            BucketContents::Sorted(b) => b.iter().filter(|n| n.status() == NodeStatus::Good).count(),
                            BucketContents::Assorted(b) => b.iter().filter(|n| n.status() == NodeStatus::Good).count(),
                        };
                        total += num_nodes;

                        if num_nodes != 0 {
                            print!("Bucket {index}: {num_nodes} | ");
                        }
                    }

                    print!("\nTotal: {total}\n\n\n");
                }
            }
            Ok(MessageType::Response(ResponseType::GetPeers(g))) => {
                tracing::info!("bip_dht: Received a GetPeersResponse...");
                let trans_id = TransactionID::from_bytes(g.transaction_id()).unwrap();
                let node = Node::as_good(g.node_id(), addr);

                {
                    let mut routing_table = self.routing_table.write().unwrap();

                    routing_table.add_node(&node);
                }

                let opt_lookup = {
                    let table_action = self.table_actions.lock().unwrap().get(&trans_id.action_id()).cloned();

                    match table_action {
                        Some(TableAction::Lookup(lookup)) => Some(lookup),
                        Some(TableAction::Refresh(_)) => {
                            tracing::error!(
                                "bip_dht: Resolved a GetPeersResponse ActionID to a \
                                    TableRefresh..."
                            );
                            None
                        }
                        Some(TableAction::Bootstrap(_, _)) => {
                            tracing::error!(
                                "bip_dht: Resolved a GetPeersResponse ActionID to a \
                                    TableBootstrap..."
                            );
                            None
                        }
                        None => {
                            tracing::error!(
                                "bip_dht: Resolved a TransactionID to a GetPeersResponse but no \
                                    action found..."
                            );
                            None
                        }
                    }
                };

                if let Some(lookup) = opt_lookup {
                    match lookup
                        .recv_response(
                            node,
                            trans_id,
                            g,
                            self.routing_table.clone(),
                            self.out_channel.clone(),
                            self.scheduled_task_sender.clone(),
                        )
                        .await
                    {
                        LookupStatus::Searching => (),
                        LookupStatus::Completed => {
                            self.broadcast_dht_event(DhtEvent::LookupCompleted(lookup.info_hash()));
                        }
                        LookupStatus::Failed => self.handle_shutdown(ShutdownCause::Unspecified),
                        LookupStatus::Values(values) => {
                            for v4_addr in values {
                                let sock_addr = SocketAddr::V4(v4_addr);
                                self.handshaker
                                    .lock()
                                    .await
                                    .connect(None, lookup.info_hash(), sock_addr)
                                    .await;
                            }
                        }
                    }
                }
            }
            Ok(MessageType::Response(ResponseType::Ping(_))) => {
                tracing::info!("bip_dht: Received a PingResponse...");

                // Yeah...we should never be getting this type of response (we never use this message)
            }
            Ok(MessageType::Response(ResponseType::AnnouncePeer(_))) => {
                tracing::info!("bip_dht: Received an AnnouncePeerResponse...");
            }
            Ok(MessageType::Error(e)) => {
                tracing::info!("bip_dht: Received an ErrorMessage...");

                tracing::warn!("bip_dht: KRPC error message from {:?}: {:?}", addr, e);
            }
            Err(e) => {
                tracing::warn!("bip_dht: Error parsing KRPC message: {:?}", e);
            }
        }
    }

    fn handle_register_sender(&self, sender: mpsc::Sender<DhtEvent>) {
        self.event_notifiers.lock().unwrap().push(sender);
    }

    fn handle_start_bootstrap(&self, routers: Vec<Router>, nodes: Vec<SocketAddr>) -> BoxFuture<'_, ()> {
        async move {
            let router_iter = routers.into_iter().filter_map(|r| r.ipv4_addr().ok().map(SocketAddr::V4));

            let mid_generator = self.aid_generator.lock().unwrap().generate();
            let action_id = mid_generator.action_id();

            let bootstrap_complete = {
                let table_bootstrap = {
                    let routing_table = self.routing_table.read().unwrap();
                    TableBootstrap::new(routing_table.node_id(), mid_generator, nodes, router_iter)
                };

                // Begin the bootstrap operation
                let bootstrap_status = table_bootstrap
                    .start_bootstrap(self.out_channel.clone(), self.scheduled_task_sender.clone())
                    .await;

                self.bootstrapping.store(true, Ordering::SeqCst);
                self.table_actions.lock().unwrap().insert(
                    action_id,
                    TableAction::Bootstrap(Arc::new(table_bootstrap), Arc::new(AtomicUsize::default())),
                );

                match bootstrap_status {
                    Ok(BootstrapStatus::Idle) => true,
                    Ok(BootstrapStatus::Bootstrapping) => false,
                    Err(BootstrapStatus::Failed) => {
                        self.handle_shutdown(ShutdownCause::Unspecified);
                        false
                    }
                    Ok(_) | Err(_) => unreachable!(),
                }
            };

            if bootstrap_complete {
                self.broadcast_bootstrap_completed(action_id).await;
            }
        }
        .boxed()
    }

    fn handle_start_lookup(&self, info_hash: InfoHash, should_announce: bool) -> BoxFuture<'_, ()> {
        async move {
            let mid_generator = self.aid_generator.lock().unwrap().generate();
            let action_id = mid_generator.action_id();

            if self.bootstrapping.load(Ordering::Acquire) {
                // Queue it up if we are currently bootstrapping
                self.future_actions
                    .lock()
                    .unwrap()
                    .push(PostBootstrapAction::Lookup(info_hash, should_announce));
            } else {
                let node_id = self.routing_table.read().unwrap().node_id();
                // Start the lookup right now if not bootstrapping
                match TableLookup::new(
                    node_id,
                    info_hash,
                    mid_generator,
                    should_announce,
                    self.routing_table.clone(),
                    self.out_channel.clone(),
                    self.scheduled_task_sender.clone(),
                )
                .await
                {
                    Some(lookup) => {
                        self.table_actions
                            .lock()
                            .unwrap()
                            .insert(action_id, TableAction::Lookup(Arc::new(lookup)));
                    }
                    None => self.handle_shutdown(ShutdownCause::Unspecified),
                }
            }
        }
        .boxed()
    }

    fn handle_shutdown(&self, cause: ShutdownCause) {
        self.broadcast_dht_event(DhtEvent::ShuttingDown(cause));
    }

    async fn handle_scheduled_task(&self, task: ScheduledTaskCheck) {
        match task {
            ScheduledTaskCheck::TableRefresh(trans_id) => {
                self.handle_check_table_refresh(trans_id).await;
            }
            ScheduledTaskCheck::BootstrapTimeout(trans_id) => {
                self.handle_check_bootstrap_timeout(trans_id).await;
            }
            ScheduledTaskCheck::LookupTimeout(trans_id) => {
                self.handle_check_lookup_timeout(trans_id).await;
            }
            ScheduledTaskCheck::LookupEndGame(trans_id) => {
                self.handle_check_lookup_endgame(trans_id).await;
            }
        }
    }

    async fn handle_check_table_refresh(&self, trans_id: TransactionID) {
        let table_actions = self.table_actions.lock().unwrap().get(&trans_id.action_id()).cloned();

        let opt_refresh_status = match table_actions {
            Some(TableAction::Refresh(refresh)) => Some(
                refresh
                    .continue_refresh(
                        self.routing_table.clone(),
                        self.out_channel.clone(),
                        self.scheduled_task_sender.clone(),
                    )
                    .await,
            ),
            Some(TableAction::Lookup(_)) => {
                tracing::error!(
                    "bip_dht: Resolved a TransactionID to a check table refresh but TableLookup \
                        found..."
                );
                None
            }
            Some(TableAction::Bootstrap(_, _)) => {
                tracing::error!(
                    "bip_dht: Resolved a TransactionID to a check table refresh but \
                        TableBootstrap found..."
                );
                None
            }
            None => {
                tracing::error!(
                    "bip_dht: Resolved a TransactionID to a check table refresh but no action \
                        found..."
                );
                None
            }
        };

        match opt_refresh_status {
            Some(RefreshStatus::Refreshing) | None => (),
            Some(RefreshStatus::Failed) => self.handle_shutdown(ShutdownCause::Unspecified),
        }
    }

    async fn handle_check_bootstrap_timeout(&self, trans_id: TransactionID) {
        let bootstrap_complete = {
            let table_actions = self.table_actions.lock().unwrap().get(&trans_id.action_id()).cloned();

            let opt_bootstrap_info = match table_actions {
                Some(TableAction::Bootstrap(bootstrap, attempts)) => Some((
                    bootstrap
                        .recv_timeout::<H>(
                            trans_id,
                            self.routing_table.clone(),
                            self.out_channel.clone(),
                            self.scheduled_task_sender.clone(),
                        )
                        .await,
                    bootstrap,
                    attempts,
                )),
                Some(TableAction::Lookup(_)) => {
                    tracing::error!(
                        "bip_dht: Resolved a TransactionID to a check table bootstrap but \
                            TableLookup found..."
                    );
                    None
                }
                Some(TableAction::Refresh(_)) => {
                    tracing::error!(
                        "bip_dht: Resolved a TransactionID to a check table bootstrap but \
                            TableRefresh found..."
                    );
                    None
                }
                None => {
                    tracing::error!(
                        "bip_dht: Resolved a TransactionID to a check table bootstrap but no \
                            action found..."
                    );
                    None
                }
            };

            match opt_bootstrap_info {
                Some((BootstrapStatus::Idle, _, _)) => true,
                Some((BootstrapStatus::Bootstrapping, _, _)) | None => false,
                Some((BootstrapStatus::Failed, _, _)) => {
                    self.handle_shutdown(ShutdownCause::Unspecified);
                    false
                }
                Some((BootstrapStatus::Completed, bootstrap, attempts)) => {
                    // Check if our bootstrap was actually good
                    if should_rebootstrap(&self.routing_table.read().unwrap()) {
                        attempt_rebootstrap(
                            bootstrap,
                            attempts,
                            self.routing_table.clone(),
                            self.out_channel.clone(),
                            self.main_task_sender.clone(),
                            self.scheduled_task_sender.clone(),
                        )
                        .await
                            == Some(false)
                    } else {
                        true
                    }
                }
            }
        };

        if bootstrap_complete {
            self.broadcast_bootstrap_completed(trans_id.action_id()).await;
        }
    }

    async fn handle_check_lookup_timeout(&self, trans_id: TransactionID) {
        let table_actions = self.table_actions.lock().unwrap().get(&trans_id.action_id()).cloned();

        let opt_lookup_info = match table_actions {
            Some(TableAction::Lookup(lookup)) => Some((
                lookup
                    .recv_timeout(
                        trans_id,
                        self.routing_table.clone(),
                        self.out_channel.clone(),
                        self.scheduled_task_sender.clone(),
                    )
                    .await,
                lookup.info_hash(),
            )),
            Some(TableAction::Bootstrap(_, _)) => {
                tracing::error!(
                    "bip_dht: Resolved a TransactionID to a check table lookup but TableBootstrap \
                        found..."
                );
                None
            }
            Some(TableAction::Refresh(_)) => {
                tracing::error!(
                    "bip_dht: Resolved a TransactionID to a check table lookup but TableRefresh \
                        found..."
                );
                None
            }
            None => {
                tracing::error!(
                    "bip_dht: Resolved a TransactionID to a check table lookup but no action \
                        found..."
                );
                None
            }
        };

        match opt_lookup_info {
            Some((LookupStatus::Searching, _)) | None => (),
            Some((LookupStatus::Completed, info_hash)) => {
                self.broadcast_dht_event(DhtEvent::LookupCompleted(info_hash));
            }
            Some((LookupStatus::Failed, _)) => self.handle_shutdown(ShutdownCause::Unspecified),
            Some((LookupStatus::Values(v), info_hash)) => {
                // Add values to handshaker
                for v4_addr in v {
                    let sock_addr = SocketAddr::V4(v4_addr);

                    self.handshaker.lock().await.connect(None, info_hash, sock_addr).await;
                }
            }
        }
    }

    async fn handle_check_lookup_endgame(&self, trans_id: TransactionID) {
        let table_actions = self.table_actions.lock().unwrap().get(&trans_id.action_id()).cloned();

        let opt_lookup_info = match table_actions {
            Some(TableAction::Lookup(lookup)) => {
                let handshaker_port = self.handshaker.lock().await.port();

                Some((
                    lookup
                        .recv_finished(handshaker_port, self.routing_table.clone(), self.out_channel.clone())
                        .await,
                    lookup.info_hash(),
                ))
            }
            Some(TableAction::Bootstrap(_, _)) => {
                tracing::error!(
                    "bip_dht: Resolved a TransactionID to a check table lookup but TableBootstrap \
                        found..."
                );
                None
            }
            Some(TableAction::Refresh(_)) => {
                tracing::error!(
                    "bip_dht: Resolved a TransactionID to a check table lookup but TableRefresh \
                        found..."
                );
                None
            }
            None => {
                tracing::error!(
                    "bip_dht: Resolved a TransactionID to a check table lookup but no action \
                        found..."
                );
                None
            }
        };

        match opt_lookup_info {
            Some((LookupStatus::Searching, _)) | None => (),
            Some((LookupStatus::Completed, info_hash)) => {
                self.broadcast_dht_event(DhtEvent::LookupCompleted(info_hash));
            }
            Some((LookupStatus::Failed, _)) => self.handle_shutdown(ShutdownCause::Unspecified),
            Some((LookupStatus::Values(v), info_hash)) => {
                // Add values to handshaker
                for v4_addr in v {
                    let sock_addr = SocketAddr::V4(v4_addr);

                    self.handshaker.lock().await.connect(None, info_hash, sock_addr).await;
                }
            }
        }
    }

    fn broadcast_dht_event(&self, event: DhtEvent) {
        self.event_notifiers
            .lock()
            .unwrap()
            .retain(|send| send.clone().try_send(event).is_ok());
    }

    async fn broadcast_bootstrap_completed(&self, action_id: ActionID) {
        // Send notification that the bootstrap has completed.
        self.broadcast_dht_event(DhtEvent::BootstrapCompleted);

        // Indicates we are out of the bootstrapping phase
        self.bootstrapping.store(false, Ordering::Release);

        // Remove the bootstrap action from our table actions
        {
            let mut table_actions = self.table_actions.lock().unwrap();
            table_actions.remove(&action_id);
        }
        // Start the post bootstrap actions.
        let mut future_actions = self.future_actions.lock().unwrap().split_off(0);
        for table_action in future_actions.drain(..) {
            match table_action {
                PostBootstrapAction::Lookup(info_hash, should_announce) => {
                    drop(table_action);
                    self.handle_start_lookup(info_hash, should_announce).await;
                }
                PostBootstrapAction::Refresh(refresh, trans_id) => {
                    {
                        let mut table_actions = self.table_actions.lock().unwrap();
                        table_actions.insert(trans_id.action_id(), TableAction::Refresh(Arc::new(*refresh)));
                    }

                    self.handle_check_table_refresh(trans_id).await;
                }
            }
        }
    }
}

// ----------------------------------------------------------------------------//

/// Attempt to rebootstrap or shutdown the dht if we have no nodes after rebootstrapping multiple time.
/// Returns None if the DHT is shutting down, Some(true) if the rebootstrap process started, Some(false) if a rebootstrap is not necessary.
fn attempt_rebootstrap(
    bootstrap: Arc<TableBootstrap>,
    attempts: Arc<AtomicUsize>,
    routing_table: Arc<RwLock<RoutingTable>>,
    out: mpsc::Sender<(Vec<u8>, SocketAddr)>,
    main_task_sender: mpsc::Sender<OneshotTask>,
    scheduled_task_sender: mpsc::Sender<ScheduledTaskCheck>,
) -> BoxFuture<'static, Option<bool>> {
    async move {
        // Increment the bootstrap counter
        let attempt = attempts.fetch_add(1, Ordering::AcqRel) + 1;

        tracing::warn!("bip_dht: Bootstrap attempt {} failed, attempting a rebootstrap...", attempt);

        // Check if we reached the maximum bootstrap attempts
        if attempt >= MAX_BOOTSTRAP_ATTEMPTS {
            if num_good_nodes(&routing_table.read().unwrap()) == 0 {
                // Failed to get any nodes in the rebootstrap attempts, shut down
                shutdown_event_loop(main_task_sender, ShutdownCause::BootstrapFailed).await;
                None
            } else {
                Some(false)
            }
        } else {
            let bootstrap_status = bootstrap.start_bootstrap(out.clone(), scheduled_task_sender.clone()).await;

            match bootstrap_status {
                Ok(BootstrapStatus::Idle) => Some(false),
                Ok(BootstrapStatus::Bootstrapping) => Some(true),
                Err(BootstrapStatus::Failed) => {
                    shutdown_event_loop(main_task_sender, ShutdownCause::Unspecified).await;
                    None
                }
                Ok(_) | Err(_) => unreachable!(),
            }
        }
    }
    .boxed()
}

/// Shut down the event loop by sending it a shutdown message with the given cause.
async fn shutdown_event_loop(mut main_task_sender: mpsc::Sender<OneshotTask>, cause: ShutdownCause) {
    if main_task_sender.send(OneshotTask::Shutdown(cause)).await.is_err() {
        tracing::error!("bip_dht: Failed to send a shutdown message to the EventLoop...");
    }
}

/// Number of good nodes in the `RoutingTable`.
fn num_good_nodes(table: &RoutingTable) -> usize {
    table
        .closest_nodes(table.node_id())
        .filter(|n| n.status() == NodeStatus::Good)
        .count()
}

/// We should rebootstrap if we have a low number of nodes.
fn should_rebootstrap(table: &RoutingTable) -> bool {
    num_good_nodes(table) <= BOOTSTRAP_GOOD_NODE_THRESHOLD
}
