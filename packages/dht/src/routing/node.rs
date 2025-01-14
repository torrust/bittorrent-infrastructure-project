// TODO: Remove when the routing table updates node's state on request/responses.
#![allow(unused)]

use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use util::bt::NodeId;
use util::test;

// TODO: Should remove as_* functions and replace them with from_requested, from_responded, etc to hide the logic
// of the nodes initial status.

// TODO: Should address the subsecond lookup paper where questionable nodes should not automatically be replaced with
// good nodes, instead, questionable nodes should be pinged twice and then become available to be replaced. This reduces
// GOOD node churn since after 15 minutes, a long lasting node could potentially be replaced by a short lived good node.
// This strategy is actually what is vaguely specified in the standard?

// TODO: Should we be storing a SocketAddr instead of a SocketAddrV4?

/// Maximum wait period before a node becomes questionable.
const MAX_LAST_SEEN_MINS: i64 = 15;

/// Maximum number of requests before a Questionable node becomes Bad.
const MAX_REFRESH_REQUESTS: usize = 2;

/// Status of the node.
/// Ordering of the enumerations is important, variants higher
/// up are considered to be less than those further down.
#[allow(clippy::module_name_repetitions)]
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Ord, PartialOrd)]
pub enum NodeStatus {
    Bad,
    Questionable,
    Good,
}

/// Node participating in the dht.
pub struct Node {
    id: NodeId,
    addr: SocketAddr,
    last_request: Arc<Mutex<Option<DateTime<Utc>>>>,
    last_response: Arc<Mutex<Option<DateTime<Utc>>>>,
    refresh_requests: Arc<AtomicUsize>,
}

impl Node {
    /// Create a new node that has recently responded to us but never requested from us.
    pub fn as_good(id: NodeId, addr: SocketAddr) -> Node {
        Node {
            id,
            addr,
            last_response: Arc::new(Mutex::new(Some(Utc::now()))),
            last_request: Arc::default(),
            refresh_requests: Arc::default(),
        }
    }

    /// Create a questionable node that has responded to us before but never requested from us.
    pub fn as_questionable(id: NodeId, addr: SocketAddr) -> Node {
        let last_response_offset = Duration::minutes(MAX_LAST_SEEN_MINS);
        // TODO: don't use test helpers in actual code!!!
        let last_response = test::travel_into_past(last_response_offset);

        Node {
            id,
            addr,
            last_response: Arc::new(Mutex::new(Some(last_response))),
            last_request: Arc::default(),
            refresh_requests: Arc::default(),
        }
    }

    /// Create a new node that has never responded to us or requested from us.
    pub fn as_bad(id: NodeId, addr: SocketAddr) -> Node {
        Node {
            id,
            addr,
            last_response: Arc::default(),
            last_request: Arc::default(),
            refresh_requests: Arc::default(),
        }
    }

    /// Record that we sent the node a request.
    pub fn local_request(&self) {
        if self.status() != NodeStatus::Good {
            let num_requests = self.refresh_requests.fetch_add(1, Ordering::SeqCst) + 1;
        }
    }

    /// Record that the node sent us a request.
    pub fn remote_request(&self) {
        *self.last_request.lock().unwrap() = Some(Utc::now());
    }

    /// Record that the node sent us a response.
    pub fn remote_response(&self) {
        *self.last_response.lock().unwrap() = Some(Utc::now());

        self.refresh_requests.store(0, Ordering::Relaxed);
    }

    pub fn id(&self) -> NodeId {
        self.id
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn encode(&self) -> [u8; 26] {
        let mut encoded = [0u8; 26];

        {
            let mut encoded_iter = encoded.iter_mut();

            // Copy the node id over
            for (src, dst) in self.id.as_ref().iter().zip(encoded_iter.by_ref()) {
                *dst = *src;
            }

            // Copy the ip address over
            match self.addr {
                SocketAddr::V4(v4) => {
                    for (src, dst) in v4.ip().octets().iter().zip(encoded_iter.by_ref()) {
                        *dst = *src;
                    }
                }
                SocketAddr::V6(_) => panic!("bip_dht: Cannot encode a SocketAddrV6..."),
            }
        }

        // Copy the port over
        let port = self.addr.port();
        encoded[24] = (port >> 8) as u8;

        #[allow(clippy::cast_possible_truncation)]
        let port = port as u8;
        encoded[25] = port;

        encoded
    }

    /// Current status of the node.
    pub fn status(&self) -> NodeStatus {
        let curr_time = Utc::now();

        match recently_responded(self, curr_time) {
            NodeStatus::Good => return NodeStatus::Good,
            NodeStatus::Bad => return NodeStatus::Bad,
            NodeStatus::Questionable => (),
        };

        recently_requested(self, curr_time)
    }
}

impl Eq for Node {}

impl PartialEq<Node> for Node {
    fn eq(&self, other: &Node) -> bool {
        self.id == other.id && self.addr == other.addr
    }
}

impl Hash for Node {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.id.hash(state);
        self.addr.hash(state);
    }
}

impl Clone for Node {
    fn clone(&self) -> Node {
        Node {
            id: self.id,
            addr: self.addr,
            last_response: self.last_response.clone(),
            last_request: self.last_request.clone(),
            refresh_requests: self.refresh_requests.clone(),
        }
    }
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        f.write_fmt(format_args!(
            "Node{{ id: {:?}, addr: {:?}, last_request: {:?}, \
                                  last_response: {:?}, refresh_requests: {:?} }}",
            self.id,
            self.addr,
            self.last_request.lock().unwrap(),
            self.last_response.lock().unwrap(),
            self.refresh_requests.load(Ordering::Relaxed)
        ))
    }
}

// TODO: Verify the two scenarios follow the specification as some cases seem questionable (pun intended), ie, a node
// responds to us once, and then requests from us but never responds to us for the duration of the session. This means they
// could stay marked as a good node even though they could ignore our requests and just sending us periodic requests
// to keep their node marked as good in our routing table...

/// First scenario where a node is good is if it has responded to one of our requests recently.
///
/// Returns the status of the node where a Questionable status means the node has responded
/// to us before, but not recently.
fn recently_responded(node: &Node, curr_time: DateTime<Utc>) -> NodeStatus {
    // Check if node has ever responded to us
    let since_response = match *node.last_response.lock().unwrap() {
        Some(response_time) => curr_time - response_time,
        None => return NodeStatus::Bad,
    };

    // Check if node has recently responded to us
    let max_last_response = Duration::minutes(MAX_LAST_SEEN_MINS);
    if since_response < max_last_response {
        NodeStatus::Good
    } else {
        NodeStatus::Questionable
    }
}

/// Second scenario where a node has ever responded to one of our requests and is good if it
/// has sent us a request recently.
///
/// Returns the final status of the node given that the first scenario found the node to be
/// Questionable.
fn recently_requested(node: &Node, curr_time: DateTime<Utc>) -> NodeStatus {
    let max_last_request = Duration::minutes(MAX_LAST_SEEN_MINS);

    // Check if the node has recently request from us
    if let Some(request_time) = *node.last_request.lock().unwrap() {
        let since_request = curr_time - request_time;

        if since_request < max_last_request {
            return NodeStatus::Good;
        }
    }

    // Check if we have request from node multiple times already without response
    if node.refresh_requests.load(Ordering::Relaxed) < MAX_REFRESH_REQUESTS {
        NodeStatus::Questionable
    } else {
        NodeStatus::Bad
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

    use chrono::Duration;
    use util::bt::NodeId;
    use util::test as bip_test;

    use crate::routing::node::{Node, NodeStatus};

    #[test]
    fn positive_encode_node() {
        let node_id = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
        let ip_addr = [127, 0, 0, 1];
        let port = 6881;

        let v4_ip = Ipv4Addr::new(ip_addr[0], ip_addr[1], ip_addr[2], ip_addr[3]);
        let sock_addr = SocketAddr::V4(SocketAddrV4::new(v4_ip, port));

        let node = Node::as_good(node_id.into(), sock_addr);

        let encoded_node = node.encode();

        #[allow(clippy::cast_possible_truncation)]
        let port_bytes = [(port >> 8) as u8, port as u8];
        for (expected, actual) in node_id
            .iter()
            .chain(ip_addr.iter())
            .chain(port_bytes.iter())
            .zip(encoded_node.iter())
        {
            assert_eq!(expected, actual);
        }
    }

    #[test]
    fn positive_as_bad() {
        let node = Node::as_bad(bip_test::dummy_node_id(), bip_test::dummy_socket_addr_v4());

        assert_eq!(node.status(), NodeStatus::Bad);
    }

    #[test]
    fn positive_as_questionable() {
        let node = Node::as_questionable(bip_test::dummy_node_id(), bip_test::dummy_socket_addr_v4());

        assert_eq!(node.status(), NodeStatus::Questionable);
    }

    #[test]
    fn positive_as_good() {
        let node = Node::as_good(bip_test::dummy_node_id(), bip_test::dummy_socket_addr_v4());

        assert_eq!(node.status(), NodeStatus::Good);
    }

    #[test]
    fn positive_response_renewal() {
        let node = Node::as_questionable(bip_test::dummy_node_id(), bip_test::dummy_socket_addr_v4());

        node.remote_response();

        assert_eq!(node.status(), NodeStatus::Good);
    }

    #[test]
    fn positive_request_renewal() {
        let node = Node::as_questionable(bip_test::dummy_node_id(), bip_test::dummy_socket_addr_v4());

        node.remote_request();

        assert_eq!(node.status(), NodeStatus::Good);
    }

    #[test]
    fn positive_node_idle() {
        let node = Node::as_good(bip_test::dummy_node_id(), bip_test::dummy_socket_addr_v4());

        let time_offset = Duration::minutes(super::MAX_LAST_SEEN_MINS);
        let idle_time = bip_test::travel_into_past(time_offset);

        *node.last_response.lock().unwrap() = (Some(idle_time));

        assert_eq!(node.status(), NodeStatus::Questionable);
    }

    #[test]
    fn positive_node_idle_requests() {
        let node = Node::as_questionable(bip_test::dummy_node_id(), bip_test::dummy_socket_addr_v4());

        for _ in 0..super::MAX_REFRESH_REQUESTS {
            node.local_request();
        }

        assert_eq!(node.status(), NodeStatus::Bad);
    }

    #[test]
    fn positive_good_status_ordering() {
        assert!(NodeStatus::Good > NodeStatus::Questionable);
        assert!(NodeStatus::Good > NodeStatus::Bad);
        assert!(NodeStatus::Good == NodeStatus::Good);
    }

    #[test]
    fn positive_questionable_status_ordering() {
        assert!(NodeStatus::Questionable > NodeStatus::Bad);
        assert!(NodeStatus::Questionable < NodeStatus::Good);
        assert!(NodeStatus::Questionable == NodeStatus::Questionable);
    }

    #[test]
    fn positive_bad_status_ordering() {
        assert!(NodeStatus::Bad < NodeStatus::Good);
        assert!(NodeStatus::Bad < NodeStatus::Questionable);
        assert!(NodeStatus::Bad == NodeStatus::Bad);
    }
}
