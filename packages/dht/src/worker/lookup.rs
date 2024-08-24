use std::collections::{HashMap, HashSet};
use std::net::{SocketAddr, SocketAddrV4};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use bencode::BRefAccess;
use futures::channel::mpsc;
use futures::channel::mpsc::SendError;
use futures::future::BoxFuture;
use futures::{FutureExt, SinkExt as _};
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration, Instant};
use util::bt::{self, InfoHash, NodeId};
use util::net;
use util::sha::ShaHash;

use crate::message::announce_peer::{AnnouncePeerRequest, ConnectPort};
use crate::message::get_peers::{CompactInfoType, GetPeersRequest, GetPeersResponse};
use crate::routing::bucket;
use crate::routing::node::{Node, NodeStatus};
use crate::routing::table::RoutingTable;
use crate::transaction::{MIDGenerator, TransactionID};
use crate::worker::ScheduledTaskCheck;

const LOOKUP_TIMEOUT_MS: u64 = 1500;
const ENDGAME_TIMEOUT_MS: u64 = 1500;

const INITIAL_PICK_NUM: usize = 4; // Alpha
const ITERATIVE_PICK_NUM: usize = 3; // Beta
const ANNOUNCE_PICK_NUM: usize = 8; // # Announces

type Distance = ShaHash;
type DistanceToBeat = ShaHash;

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, PartialEq, Eq)]
pub enum LookupStatus {
    Searching,
    Values(Vec<SocketAddrV4>),
    Completed,
    Failed,
}

#[allow(clippy::module_name_repetitions)]
pub struct TableLookup {
    table_id: NodeId,
    target_id: InfoHash,
    in_endgame: AtomicBool,
    recv_values: AtomicBool,
    id_generator: Mutex<MIDGenerator>,
    will_announce: bool,
    active_lookups: Mutex<HashMap<TransactionID, (DistanceToBeat, Instant)>>,
    announce_tokens: Mutex<HashMap<Node, Vec<u8>>>,
    requested_nodes: Mutex<HashSet<Node>>,
    all_sorted_nodes: Mutex<Vec<(Distance, Node, Arc<AtomicBool>)>>,
    tasks: Arc<Mutex<JoinSet<Result<(), SendError>>>>,
}

impl TableLookup {
    pub fn new<'a>(
        table_id: NodeId,
        target_id: InfoHash,
        id_generator: MIDGenerator,
        will_announce: bool,
        table: Arc<RwLock<RoutingTable>>,
        out: mpsc::Sender<(Vec<u8>, SocketAddr)>,
        scheduled_task_sender: mpsc::Sender<ScheduledTaskCheck>,
    ) -> BoxFuture<'a, Option<TableLookup>> {
        async move {
            let all_sorted_nodes = Mutex::new(Vec::with_capacity(bucket::MAX_BUCKET_SIZE));

            for node in table
                .read()
                .unwrap()
                .closest_nodes(target_id)
                .filter(|n| n.status() == NodeStatus::Good)
                .take(bucket::MAX_BUCKET_SIZE)
            {
                insert_sorted_node(&all_sorted_nodes, target_id, node.clone(), false);
            }

            let initial_pick_nodes = pick_initial_nodes(all_sorted_nodes.lock().unwrap().iter_mut());
            let initial_pick_nodes_filtered = initial_pick_nodes.iter().filter(|&&(_, good)| good).map(|(node, _)| {
                let distance_to_beat = node.id() ^ target_id;

                (node, distance_to_beat)
            });

            let table_lookup = TableLookup {
                table_id,
                target_id,
                in_endgame: AtomicBool::default(),
                recv_values: AtomicBool::default(),
                id_generator: Mutex::new(id_generator),
                will_announce,
                all_sorted_nodes,
                announce_tokens: Mutex::new(HashMap::new()),
                requested_nodes: Mutex::new(HashSet::new()),
                active_lookups: Mutex::new(HashMap::with_capacity(INITIAL_PICK_NUM)),
                tasks: Arc::default(),
            };

            if table_lookup
                .start_request_round(
                    initial_pick_nodes_filtered,
                    table.clone(),
                    out.clone(),
                    &scheduled_task_sender,
                )
                .await
                == LookupStatus::Failed
            {
                None
            } else {
                Some(table_lookup)
            }
        }
        .boxed()
    }

    pub fn info_hash(&self) -> InfoHash {
        self.target_id
    }

    pub async fn recv_response<B>(
        &self,
        node: Node,
        trans_id: TransactionID,
        msg: GetPeersResponse<'_, B>,
        table: Arc<RwLock<RoutingTable>>,
        out: mpsc::Sender<(Vec<u8>, SocketAddr)>,
        scheduled_task_sender: mpsc::Sender<ScheduledTaskCheck>,
    ) -> LookupStatus
    where
        B: BRefAccess<BType = B> + Clone,
        B::BType: PartialEq + Eq + core::hash::Hash + std::fmt::Debug,
    {
        let Some((dist_to_beat, _)) = self.active_lookups.lock().unwrap().remove(&trans_id) else {
            tracing::warn!(
                "bip_dht: Received expired/unsolicited node response for an active table \
                   lookup..."
            );
            return self.current_lookup_status();
        };

        if let Some(token) = msg.token() {
            self.announce_tokens.lock().unwrap().insert(node, token.to_vec());
        }

        let (opt_values, opt_nodes) = match msg.info_type() {
            CompactInfoType::Nodes(n) => (None, Some(n)),
            CompactInfoType::Values(v) => {
                self.recv_values.store(true, Ordering::Relaxed);
                (Some(v.into_iter().collect()), None)
            }
            CompactInfoType::Both(n, v) => (Some(v.into_iter().collect()), Some(n)),
        };

        let (iterate_nodes, next_dist_to_beat) = if let Some(nodes) = opt_nodes {
            #[allow(clippy::mutable_key_type)]
            let requested_nodes = &self.requested_nodes;

            let already_requested = |node_info: &(NodeId, SocketAddrV4)| {
                let node = Node::as_questionable(node_info.0, SocketAddr::V4(node_info.1));

                !requested_nodes.lock().unwrap().contains(&node)
            };

            let next_dist_to_beat = nodes
                .into_iter()
                .filter(&already_requested)
                .fold(dist_to_beat, |closest, (id, _)| {
                    let distance = self.target_id ^ id;

                    if distance < closest {
                        distance
                    } else {
                        closest
                    }
                });

            let iterate_nodes = if next_dist_to_beat < dist_to_beat {
                let iterate_nodes = pick_iterate_nodes(nodes.into_iter().filter(&already_requested), self.target_id);

                for (id, v4_addr) in nodes {
                    let addr = SocketAddr::V4(v4_addr);
                    let node = Node::as_questionable(id, addr);
                    let will_ping = iterate_nodes.iter().any(|(n, _)| n == &node);

                    insert_sorted_node(&self.all_sorted_nodes, self.target_id, node, will_ping);
                }

                Some(iterate_nodes)
            } else {
                for (id, v4_addr) in nodes {
                    let addr = SocketAddr::V4(v4_addr);
                    let node = Node::as_questionable(id, addr);

                    insert_sorted_node(&self.all_sorted_nodes, self.target_id, node, false);
                }

                None
            };

            (iterate_nodes, next_dist_to_beat)
        } else {
            (None, dist_to_beat)
        };

        if !self.in_endgame.load(Ordering::Relaxed) {
            if let Some(ref nodes) = iterate_nodes {
                let filtered_nodes = nodes.iter().filter(|&&(_, good)| good).map(|(n, _)| (n, next_dist_to_beat));
                if self
                    .start_request_round(filtered_nodes, table.clone(), out.clone(), &scheduled_task_sender)
                    .await
                    == LookupStatus::Failed
                {
                    return LookupStatus::Failed;
                }
            }

            if self.active_lookups.lock().unwrap().is_empty()
                && self.start_endgame_round(table, out.clone(), scheduled_task_sender).await == LookupStatus::Failed
            {
                return LookupStatus::Failed;
            }
        }

        match opt_values {
            Some(values) => LookupStatus::Values(values),
            None => self.current_lookup_status(),
        }
    }

    pub async fn recv_timeout(
        &self,
        trans_id: TransactionID,
        table: Arc<RwLock<RoutingTable>>,
        out: mpsc::Sender<(Vec<u8>, SocketAddr)>,
        scheduled_task_sender: mpsc::Sender<ScheduledTaskCheck>,
    ) -> LookupStatus {
        if self.active_lookups.lock().unwrap().remove(&trans_id).is_none() {
            tracing::warn!(
                "bip_dht: Received expired/unsolicited node timeout for an active table \
                   lookup..."
            );
            return self.current_lookup_status();
        }

        if !self.in_endgame.load(Ordering::Relaxed)
            && self.active_lookups.lock().unwrap().is_empty()
            && self.start_endgame_round(table, out.clone(), scheduled_task_sender).await == LookupStatus::Failed
        {
            return LookupStatus::Failed;
        }

        self.current_lookup_status()
    }

    pub async fn recv_finished(
        &self,
        handshake_port: u16,
        table: Arc<RwLock<RoutingTable>>,
        mut out: mpsc::Sender<(Vec<u8>, SocketAddr)>,
    ) -> LookupStatus {
        let mut fatal_error = false;

        if self.will_announce {
            #[allow(clippy::mutable_key_type)]
            let announce_tokens = &self.announce_tokens;

            let mut node_announces = Vec::new();

            for (_, node, _) in self
                .all_sorted_nodes
                .lock()
                .unwrap()
                .iter()
                .filter(|&(_, node, _)| announce_tokens.lock().unwrap().contains_key(node))
                .take(ANNOUNCE_PICK_NUM)
                .cloned()
            {
                let trans_id = self.id_generator.lock().unwrap().generate();
                let announce_tokens = announce_tokens.lock().unwrap();
                let token = announce_tokens.get(&node).unwrap();

                let announce_peer_req = AnnouncePeerRequest::new(
                    trans_id.as_ref(),
                    self.table_id,
                    self.target_id,
                    token.as_ref(),
                    ConnectPort::Explicit(handshake_port),
                );
                let announce_peer_msg = announce_peer_req.encode();

                node_announces.push((node, announce_peer_msg));
            }

            for (node, announce_peer_msg) in node_announces {
                if out.send((announce_peer_msg, node.addr())).await.is_err() {
                    tracing::error!(
                        "bip_dht: TableLookup announce request failed to send through the out \
                        channel..."
                    );
                    fatal_error = true;
                }

                let routing_table = table.read().unwrap();
                if !fatal_error {
                    if let Some(n) = routing_table.find_node(&node) {
                        n.local_request();
                    }
                }
            }
        }

        self.active_lookups.lock().unwrap().clear();
        self.in_endgame.store(false, Ordering::Relaxed);

        if fatal_error {
            LookupStatus::Failed
        } else {
            self.current_lookup_status()
        }
    }

    fn current_lookup_status(&self) -> LookupStatus {
        if self.in_endgame.load(Ordering::Relaxed) || !self.active_lookups.lock().unwrap().is_empty() {
            LookupStatus::Searching
        } else {
            LookupStatus::Completed
        }
    }

    async fn start_request_round<'a, I>(
        &self,
        nodes: I,
        table: Arc<RwLock<RoutingTable>>,
        mut out: mpsc::Sender<(Vec<u8>, SocketAddr)>,
        scheduled_task_sender: &mpsc::Sender<ScheduledTaskCheck>,
    ) -> LookupStatus
    where
        I: Iterator<Item = (&'a Node, DistanceToBeat)>,
    {
        let mut messages_sent = 0;
        for (node, dist_to_beat) in nodes {
            let trans_id = self.id_generator.lock().unwrap().generate();

            let timeout = Instant::now() + Duration::from_millis(LOOKUP_TIMEOUT_MS);

            self.active_lookups.lock().unwrap().insert(trans_id, (dist_to_beat, timeout));

            let get_peers_msg = GetPeersRequest::new(trans_id.as_ref(), self.table_id, self.target_id).encode();
            if out.send((get_peers_msg, node.addr())).await.is_err() {
                tracing::error!("bip_dht: Could not send a lookup message through the channel...");
                return LookupStatus::Failed;
            }

            self.requested_nodes.lock().unwrap().insert(node.clone());

            let routing_table = table.read().unwrap();

            if let Some(n) = routing_table.find_node(node) {
                n.local_request();
            }

            messages_sent += 1;

            // Schedule a timeout check
            let mut this_scheduled_task_sender = scheduled_task_sender.clone();
            self.tasks.lock().unwrap().spawn(async move {
                sleep(Duration::from_millis(LOOKUP_TIMEOUT_MS)).await;

                match this_scheduled_task_sender
                    .send(ScheduledTaskCheck::LookupTimeout(trans_id))
                    .await
                {
                    Ok(()) => {
                        tracing::debug!("sent scheduled lookup timeout");
                        Ok(())
                    }
                    Err(e) => {
                        tracing::debug!("error sending scheduled lookup timeout: {e}");
                        Err(e)
                    }
                }
            });
        }

        if messages_sent == 0 {
            self.active_lookups.lock().unwrap().clear();
            LookupStatus::Completed
        } else {
            LookupStatus::Searching
        }
    }

    async fn start_endgame_round(
        &self,
        table: Arc<RwLock<RoutingTable>>,
        mut out: mpsc::Sender<(Vec<u8>, SocketAddr)>,
        _scheduled_task_sender: mpsc::Sender<ScheduledTaskCheck>,
    ) -> LookupStatus {
        self.in_endgame.store(true, Ordering::SeqCst);

        let timeout = Instant::now() + Duration::from_millis(ENDGAME_TIMEOUT_MS);

        let mut endgame_messages = Vec::new();

        if !self.recv_values.load(Ordering::SeqCst) {
            {
                let all_nodes = self.all_sorted_nodes.lock().unwrap();
                for node_info in all_nodes.iter().filter(|(_, _, req)| !req.load(Ordering::Acquire)) {
                    let (node_dist, node, req) = node_info;

                    let trans_id = self.id_generator.lock().unwrap().generate();

                    self.active_lookups.lock().unwrap().insert(trans_id, (*node_dist, timeout));

                    let get_peers_msg = GetPeersRequest::new(trans_id.as_ref(), self.table_id, self.target_id).encode();

                    endgame_messages.push((node.clone(), get_peers_msg, req.clone()));
                }
            }

            for (node, get_peers_msg, req) in endgame_messages {
                if out.send((get_peers_msg, node.addr())).await.is_err() {
                    tracing::error!("bip_dht: Could not send an endgame message through the channel...");
                    return LookupStatus::Failed;
                }

                let routing_table = table.read().unwrap();

                if let Some(n) = routing_table.find_node(&node) {
                    n.local_request();
                }

                req.store(true, Ordering::Release);
            }
        }

        LookupStatus::Searching
    }
}

/// Picks a number of nodes from the sorted distance iterator to ping on the first round.
fn pick_initial_nodes<'a, I>(sorted_nodes: I) -> [(Node, bool); INITIAL_PICK_NUM]
where
    I: Iterator<Item = &'a mut (Distance, Node, Arc<AtomicBool>)>,
{
    let dummy_id = [0u8; bt::NODE_ID_LEN].into();
    let default = (Node::as_bad(dummy_id, net::default_route_v4()), false);

    let mut pick_nodes = [default.clone(), default.clone(), default.clone(), default.clone()];
    for (src, dst) in sorted_nodes.zip(pick_nodes.iter_mut()) {
        dst.0 = src.1.clone();
        dst.1 = true;

        // Mark that the node has been requested from
        src.2.store(true, Ordering::Relaxed);
    }

    pick_nodes
}

fn pick_iterate_nodes<I>(unsorted_nodes: I, target_id: InfoHash) -> [(Node, bool); ITERATIVE_PICK_NUM]
where
    I: Iterator<Item = (NodeId, SocketAddrV4)>,
{
    let dummy_id = [0u8; bt::NODE_ID_LEN].into();
    let default = (Node::as_bad(dummy_id, net::default_route_v4()), false);

    let mut pick_nodes = [default.clone(), default.clone(), default.clone()];
    for (id, v4_addr) in unsorted_nodes {
        let addr = SocketAddr::V4(v4_addr);
        let node = Node::as_questionable(id, addr);

        insert_closest_nodes(&mut pick_nodes, target_id, node);
    }

    pick_nodes
}

fn insert_closest_nodes(nodes: &mut [(Node, bool)], target_id: InfoHash, new_node: Node) {
    let new_distance = target_id ^ new_node.id();

    for &mut (ref mut old_node, ref mut used) in &mut *nodes {
        if *used {
            let old_distance = target_id ^ old_node.id();

            if new_distance < old_distance {
                *old_node = new_node;
                return;
            }
        } else {
            *old_node = new_node;
            *used = true;
            return;
        }
    }
}

fn insert_sorted_node(nodes: &Mutex<Vec<(Distance, Node, Arc<AtomicBool>)>>, target: InfoHash, node: Node, pinged: bool) {
    let mut nodes = nodes.lock().unwrap();
    let node_id = node.id();
    let node_dist = target ^ node_id;

    let search_result = nodes.binary_search_by(|&(dist, _, _)| dist.cmp(&node_dist));
    match search_result {
        Ok(dup_index) => {
            if nodes[dup_index].1 != node {
                nodes.insert(dup_index, (node_dist, node, Arc::new(AtomicBool::new(pinged))));
            }
        }
        Err(ins_index) => nodes.insert(ins_index, (node_dist, node, Arc::new(AtomicBool::new(pinged)))),
    };
}
