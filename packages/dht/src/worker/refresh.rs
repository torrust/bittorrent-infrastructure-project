use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use futures::channel::mpsc::{self, SendError};
use futures::SinkExt as _;
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};
use util::bt::{self, NodeId};

use crate::message::find_node::FindNodeRequest;
use crate::routing::node::NodeStatus;
use crate::routing::table::{self, RoutingTable};
use crate::transaction::MIDGenerator;
use crate::worker::ScheduledTaskCheck;

const REFRESH_INTERVAL_TIMEOUT: u64 = 6000;

#[allow(clippy::module_name_repetitions)]
pub enum RefreshStatus {
    /// Refresh is in progress.
    Refreshing,
    /// Refresh failed in a fatal way.
    Failed,
}

#[allow(clippy::module_name_repetitions)]
pub struct TableRefresh {
    id_generator: Mutex<MIDGenerator>,
    curr_refresh_bucket: AtomicUsize,
    tasks: Arc<Mutex<JoinSet<Result<(), SendError>>>>,
}

impl TableRefresh {
    pub fn new(id_generator: MIDGenerator) -> TableRefresh {
        TableRefresh {
            id_generator: Mutex::new(id_generator),
            curr_refresh_bucket: AtomicUsize::default(),
            tasks: Arc::default(),
        }
    }

    pub async fn continue_refresh(
        &self,
        table: Arc<RwLock<RoutingTable>>,
        mut out: mpsc::Sender<(Vec<u8>, SocketAddr)>,
        mut scheduled_task_sender: mpsc::Sender<ScheduledTaskCheck>,
    ) -> RefreshStatus {
        let refresh_bucket = match self.curr_refresh_bucket.load(Ordering::Relaxed) {
            table::MAX_BUCKETS => {
                self.curr_refresh_bucket.store(0, Ordering::Relaxed);
                0
            }
            refresh_bucket => refresh_bucket,
        };

        let (node, target_id, node_id) = {
            let routing_table = table.read().unwrap();
            let node_id = routing_table.node_id();
            let target_id = flip_id_bit_at_index(node_id, refresh_bucket);
            let node = routing_table
                .closest_nodes(target_id)
                .find(|n| n.status() == NodeStatus::Questionable)
                .cloned();

            tracing::info!("bip_dht: Performing a refresh for bucket {}", refresh_bucket);

            (node, target_id, node_id)
        };

        // Ping the closest questionable node
        if let Some(node) = node {
            // Generate a transaction id for the request
            let trans_id = self.id_generator.lock().unwrap().generate();

            // Construct the message
            let find_node_req = FindNodeRequest::new(trans_id.as_ref(), node_id, target_id);
            let find_node_msg = find_node_req.encode();

            // Send the message
            if out.send((find_node_msg, node.addr())).await.is_err() {
                tracing::error!(
                    "bip_dht: TableRefresh failed to send a refresh message to the out \
                        channel..."
                );
                return RefreshStatus::Failed;
            }

            // Mark that we requested from the node
            node.local_request();
        }

        // Generate a dummy transaction id (only the action id will be used)
        let trans_id = self.id_generator.lock().unwrap().generate();

        // Start a timer for the next refresh
        self.tasks.lock().unwrap().spawn(async move {
            sleep(Duration::from_millis(REFRESH_INTERVAL_TIMEOUT)).await;

            match scheduled_task_sender.send(ScheduledTaskCheck::TableRefresh(trans_id)).await {
                Ok(()) => {
                    tracing::debug!("sent scheduled refresh timeout");
                    Ok(())
                }
                Err(e) => {
                    tracing::debug!("error sending scheduled refresh timeout: {e}");
                    Err(e)
                }
            }
        });

        self.curr_refresh_bucket.fetch_add(1, Ordering::SeqCst);

        RefreshStatus::Refreshing
    }
}

/// Panics if index is out of bounds.
fn flip_id_bit_at_index(node_id: NodeId, index: usize) -> NodeId {
    let mut id_bytes: [u8; bt::NODE_ID_LEN] = node_id.into();
    let (byte_index, bit_index) = (index / 8, index % 8);

    let actual_bit_index = 7 - bit_index;
    id_bytes[byte_index] ^= 1 << actual_bit_index;

    id_bytes.into()
}
