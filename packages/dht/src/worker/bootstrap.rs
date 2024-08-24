use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use futures::channel::mpsc::{self, SendError};
use futures::future::BoxFuture;
use futures::{FutureExt as _, SinkExt as _};
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};
use util::bt::{self, NodeId};

use crate::handshaker_trait::HandshakerTrait;
use crate::message::find_node::FindNodeRequest;
use crate::routing::bucket::Bucket;
use crate::routing::node::{Node, NodeStatus};
use crate::routing::table::{self, BucketContents, RoutingTable};
use crate::transaction::{MIDGenerator, TransactionID};
use crate::worker::ScheduledTaskCheck;

const BOOTSTRAP_INITIAL_TIMEOUT: u64 = 2500;
const BOOTSTRAP_NODE_TIMEOUT: u64 = 500;

const BOOTSTRAP_PINGS_PER_BUCKET: usize = 8;

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, PartialEq, Eq)]
pub enum BootstrapStatus {
    /// Bootstrap has been finished.
    Idle,
    /// Bootstrap is in progress.
    Bootstrapping,
    /// Bootstrap just finished.
    Completed,
    /// Bootstrap failed in a fatal way.
    Failed,
}

#[allow(clippy::module_name_repetitions)]
pub struct TableBootstrap {
    table_id: NodeId,
    id_generator: Mutex<MIDGenerator>,
    starting_nodes: Vec<SocketAddr>,
    active_messages: Mutex<HashMap<TransactionID, tokio::time::Instant>>,
    starting_routers: HashSet<SocketAddr>,
    curr_bootstrap_bucket: AtomicUsize,
    tasks: Arc<Mutex<JoinSet<Result<(), SendError>>>>,
}

impl TableBootstrap {
    pub fn new<I>(table_id: NodeId, id_generator: MIDGenerator, nodes: Vec<SocketAddr>, routers: I) -> TableBootstrap
    where
        I: Iterator<Item = SocketAddr>,
    {
        let router_filter: HashSet<SocketAddr> = routers.collect();

        TableBootstrap {
            table_id,
            id_generator: Mutex::new(id_generator),
            starting_nodes: nodes,
            starting_routers: router_filter,
            active_messages: Mutex::default(),
            curr_bootstrap_bucket: AtomicUsize::default(),
            tasks: Arc::default(),
        }
    }

    pub async fn start_bootstrap(
        &self,
        mut out: mpsc::Sender<(Vec<u8>, SocketAddr)>,
        mut scheduled_task_sender: mpsc::Sender<ScheduledTaskCheck>,
    ) -> Result<BootstrapStatus, BootstrapStatus> {
        // Reset the bootstrap state
        self.active_messages.lock().unwrap().clear();
        self.curr_bootstrap_bucket.store(0, Ordering::Relaxed);

        // Generate transaction id for the initial bootstrap messages
        let trans_id = self.id_generator.lock().unwrap().generate();

        // Set a timer to begin the actual bootstrap
        let abort = self.tasks.lock().unwrap().spawn(async move {
            sleep(Duration::from_millis(BOOTSTRAP_INITIAL_TIMEOUT)).await;

            match scheduled_task_sender
                .send(ScheduledTaskCheck::BootstrapTimeout(trans_id))
                .await
            {
                Ok(()) => {
                    tracing::debug!("sent scheduled bootstrap timeout");
                    Ok(())
                }
                Err(e) => {
                    tracing::debug!("error sending scheduled bootstrap timeout: {e}");
                    Err(e)
                }
            }
        });

        // Insert the timeout into the active bootstraps just so we can check if a response was valid (and begin the bucket bootstraps)
        self.active_messages.lock().unwrap().insert(
            trans_id,
            tokio::time::Instant::now() + Duration::from_millis(BOOTSTRAP_INITIAL_TIMEOUT),
        );

        let find_node_msg = FindNodeRequest::new(trans_id.as_ref(), self.table_id, self.table_id).encode();
        // Ping all initial routers and nodes
        for addr in self.starting_routers.iter().chain(self.starting_nodes.iter()) {
            if out.send((find_node_msg.clone(), *addr)).await.is_err() {
                tracing::error!("bip_dht: Failed to send bootstrap message to router through channel...");
                abort.abort();
                return Err(BootstrapStatus::Failed);
            }
        }

        Ok(self.current_bootstrap_status())
    }

    pub fn is_router(&self, addr: &SocketAddr) -> bool {
        self.starting_routers.contains(addr)
    }

    pub async fn recv_response<H>(
        &self,
        trans_id: TransactionID,
        table: Arc<RwLock<RoutingTable>>,
        out: mpsc::Sender<(Vec<u8>, SocketAddr)>,
        scheduled_task_sender: mpsc::Sender<ScheduledTaskCheck>,
    ) -> BootstrapStatus
    where
        H: HandshakerTrait + 'static,
    {
        // Process the message transaction id
        let _timeout = if let Some(t) = self.active_messages.lock().unwrap().get(&trans_id) {
            *t
        } else {
            tracing::warn!(
                "bip_dht: Received expired/unsolicited node response for an active table \
                   bootstrap..."
            );
            return self.current_bootstrap_status();
        };

        // If this response was from the initial bootstrap, we don't want to clear the timeout or remove
        // the token from the map as we want to wait until the proper timeout has been triggered before starting
        if self.curr_bootstrap_bucket.load(Ordering::Acquire) != 0 {
            // Message was not from the initial ping
            // Remove the token from the mapping
            self.active_messages.lock().unwrap().remove(&trans_id);
        }

        // Check if we need to bootstrap on the next bucket
        if self.active_messages.lock().unwrap().is_empty() {
            return self.bootstrap_next_bucket::<H>(table, out, scheduled_task_sender).await;
        }

        self.current_bootstrap_status()
    }

    pub async fn recv_timeout<H>(
        &self,
        trans_id: TransactionID,
        table: Arc<RwLock<RoutingTable>>,
        out: mpsc::Sender<(Vec<u8>, SocketAddr)>,
        scheduled_task_sender: mpsc::Sender<ScheduledTaskCheck>,
    ) -> BootstrapStatus
    where
        H: HandshakerTrait + 'static,
    {
        if self.active_messages.lock().unwrap().remove(&trans_id).is_none() {
            tracing::warn!(
                "bip_dht: Received expired/unsolicited node timeout for an active table \
                   bootstrap..."
            );
            return self.current_bootstrap_status();
        }

        // Check if we need to bootstrap on the next bucket
        if self.active_messages.lock().unwrap().is_empty() {
            return self.bootstrap_next_bucket::<H>(table, out, scheduled_task_sender).await;
        }

        self.current_bootstrap_status()
    }

    // Returns true if there are more buckets to bootstrap, false otherwise
    fn bootstrap_next_bucket<H>(
        &self,
        table: Arc<RwLock<RoutingTable>>,
        out: mpsc::Sender<(Vec<u8>, SocketAddr)>,
        scheduled_task_sender: mpsc::Sender<ScheduledTaskCheck>,
    ) -> BoxFuture<'_, BootstrapStatus>
    where
        H: HandshakerTrait + 'static,
    {
        async move {
            let bootstrap_bucket = self.curr_bootstrap_bucket.load(Ordering::Relaxed);

            let target_id = flip_id_bit_at_index(self.table_id, bootstrap_bucket);

            // Get the optimal iterator to bootstrap the current bucket
            if bootstrap_bucket == 0 || bootstrap_bucket == 1 {
                let questionable_nodes: Vec<Node> = table
                    .read()
                    .unwrap()
                    .closest_nodes(target_id)
                    .filter(|n| n.status() == NodeStatus::Questionable)
                    .cloned()
                    .collect();

                self.send_bootstrap_requests::<H, _>(
                    questionable_nodes.iter(),
                    target_id,
                    table.clone(),
                    out.clone(),
                    scheduled_task_sender,
                )
                .await
            } else {
                let questionable_nodes: Vec<Node> = {
                    let routing_table = table.read().unwrap();
                    let mut buckets = routing_table.buckets().skip(bootstrap_bucket - 2);
                    let dummy_bucket = Bucket::new();

                    // Sloppy probabilities of our target node residing at the node
                    let percent_25_bucket = if let Some(bucket) = buckets.next() {
                        match bucket {
                            BucketContents::Empty => dummy_bucket.iter(),
                            BucketContents::Sorted(b) | BucketContents::Assorted(b) => b.iter(),
                        }
                    } else {
                        dummy_bucket.iter()
                    };
                    let percent_50_bucket = if let Some(bucket) = buckets.next() {
                        match bucket {
                            BucketContents::Empty => dummy_bucket.iter(),
                            BucketContents::Sorted(b) | BucketContents::Assorted(b) => b.iter(),
                        }
                    } else {
                        dummy_bucket.iter()
                    };
                    let percent_100_bucket = if let Some(bucket) = buckets.next() {
                        match bucket {
                            BucketContents::Empty => dummy_bucket.iter(),
                            BucketContents::Sorted(b) | BucketContents::Assorted(b) => b.iter(),
                        }
                    } else {
                        dummy_bucket.iter()
                    };

                    // TODO: Figure out why chaining them in reverse gives us more total nodes on average, perhaps it allows us to fill up the lower
                    // buckets faster at the cost of less nodes in the higher buckets (since lower buckets are very easy to fill)...Although it should
                    // even out since we are stagnating buckets, so doing it in reverse may make sense since on the 3rd iteration, it allows us to ping
                    // questionable nodes in our first buckets right off the bat.
                    percent_25_bucket
                        .chain(percent_50_bucket)
                        .chain(percent_100_bucket)
                        .filter(|n| n.status() == NodeStatus::Questionable)
                        .cloned()
                        .collect()
                };

                self.send_bootstrap_requests::<H, _>(
                    questionable_nodes.iter(),
                    target_id,
                    table.clone(),
                    out.clone(),
                    scheduled_task_sender,
                )
                .await
            }
        }
        .boxed()
    }

    async fn send_bootstrap_requests<'a, H, I>(
        &self,
        nodes: I,
        target_id: NodeId,
        table: Arc<RwLock<RoutingTable>>,
        mut out: mpsc::Sender<(Vec<u8>, SocketAddr)>,
        scheduled_task_sender: mpsc::Sender<ScheduledTaskCheck>,
    ) -> BootstrapStatus
    where
        I: Iterator<Item = &'a Node>,
        H: HandshakerTrait + 'static,
    {
        let bootstrap_bucket = self.curr_bootstrap_bucket.load(Ordering::Relaxed);

        tracing::info!("bip_dht: bootstrap::send_bootstrap_requests {}", bootstrap_bucket);

        let mut messages_sent = 0;

        for node in nodes.take(BOOTSTRAP_PINGS_PER_BUCKET) {
            // Generate a transaction id
            let trans_id = self.id_generator.lock().unwrap().generate();
            let find_node_msg = FindNodeRequest::new(trans_id.as_ref(), self.table_id, target_id).encode();

            // Add a timeout for the node
            let timeout = tokio::time::Instant::now() + Duration::from_millis(BOOTSTRAP_NODE_TIMEOUT);

            // Send the message to the node
            if out.send((find_node_msg, node.addr())).await.is_err() {
                tracing::error!("bip_dht: Could not send a bootstrap message through the channel...");
                return BootstrapStatus::Failed;
            }

            // Mark that we requested from the node
            node.local_request();

            // Create an entry for the timeout in the map
            self.active_messages.lock().unwrap().insert(trans_id, timeout);

            messages_sent += 1;

            // Schedule a timeout check
            let mut this_scheduled_task_sender = scheduled_task_sender.clone();
            self.tasks.lock().unwrap().spawn(async move {
                sleep(Duration::from_millis(BOOTSTRAP_INITIAL_TIMEOUT)).await;

                match this_scheduled_task_sender
                    .send(ScheduledTaskCheck::BootstrapTimeout(trans_id))
                    .await
                {
                    Ok(()) => {
                        tracing::debug!("sent scheduled bootstrap timeout");
                        Ok(())
                    }
                    Err(e) => {
                        tracing::debug!("error sending scheduled bootstrap timeout: {e}");
                        Err(e)
                    }
                }
            });
        }

        let bootstrap_bucket = self.curr_bootstrap_bucket.fetch_add(1, Ordering::AcqRel) + 1;

        if (bootstrap_bucket) == table::MAX_BUCKETS {
            BootstrapStatus::Completed
        } else if messages_sent == 0 {
            self.bootstrap_next_bucket::<H>(table, out, scheduled_task_sender).await
        } else {
            BootstrapStatus::Bootstrapping
        }
    }

    fn current_bootstrap_status(&self) -> BootstrapStatus {
        let bootstrap_bucket = self.curr_bootstrap_bucket.load(Ordering::Relaxed);

        if bootstrap_bucket == table::MAX_BUCKETS || self.active_messages.lock().unwrap().is_empty() {
            BootstrapStatus::Idle
        } else {
            BootstrapStatus::Bootstrapping
        }
    }
}

/// Panics if index is out of bounds.
/// TODO: Move into `bip_util` crate
fn flip_id_bit_at_index(node_id: NodeId, index: usize) -> NodeId {
    let mut id_bytes: [u8; bt::NODE_ID_LEN] = node_id.into();
    let (byte_index, bit_index) = (index / 8, index % 8);

    let actual_bit_index = 7 - bit_index;
    id_bytes[byte_index] ^= 1 << actual_bit_index;

    id_bytes.into()
}
