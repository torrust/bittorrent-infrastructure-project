use std::time::Duration;

use futures::sink::Sink;
use futures::{Stream, TryStream};

use super::{ManagedMessage, PeerManager};

const DEFAULT_PEER_CAPACITY: usize = 1000;
const DEFAULT_SINK_BUFFER_CAPACITY: usize = 100;
const DEFAULT_STREAM_BUFFER_CAPACITY: usize = 100;
const DEFAULT_HEARTBEAT_INTERVAL_MILLIS: u64 = 60 * 1000;
const DEFAULT_HEARTBEAT_TIMEOUT_MILLIS: u64 = 2 * 60 * 1000;

/// Builder for configuring a `PeerManager`.
#[allow(clippy::module_name_repetitions)]
#[derive(Default, Copy, Clone)]
pub struct PeerManagerBuilder {
    peer_capacity: usize,
    sink_buffer_capacity: usize,
    stream_buffer_capacity: usize,
    heartbeat_interval: Duration,
    heartbeat_timeout: Duration,
}

impl PeerManagerBuilder {
    /// Creates a new `PeerManagerBuilder` with default values.
    #[must_use]
    pub fn new() -> PeerManagerBuilder {
        PeerManagerBuilder {
            peer_capacity: DEFAULT_PEER_CAPACITY,
            sink_buffer_capacity: DEFAULT_SINK_BUFFER_CAPACITY,
            stream_buffer_capacity: DEFAULT_STREAM_BUFFER_CAPACITY,
            heartbeat_interval: Duration::from_millis(DEFAULT_HEARTBEAT_INTERVAL_MILLIS),
            heartbeat_timeout: Duration::from_millis(DEFAULT_HEARTBEAT_TIMEOUT_MILLIS),
        }
    }

    /// Sets the maximum number of peers that can be managed.
    #[must_use]
    pub fn with_peer_capacity(mut self, capacity: usize) -> PeerManagerBuilder {
        self.peer_capacity = capacity;
        self
    }

    /// Sets the capacity of the sink buffer for pending sent messages.
    #[must_use]
    pub fn with_sink_buffer_capacity(mut self, capacity: usize) -> PeerManagerBuilder {
        self.sink_buffer_capacity = capacity;
        self
    }

    /// Sets the capacity of the stream buffer for pending received messages.
    #[must_use]
    pub fn with_stream_buffer_capacity(mut self, capacity: usize) -> PeerManagerBuilder {
        self.stream_buffer_capacity = capacity;
        self
    }

    /// Sets the interval at which keep-alive messages are sent.
    #[must_use]
    pub fn with_heartbeat_interval(mut self, interval: Duration) -> PeerManagerBuilder {
        self.heartbeat_interval = interval;
        self
    }

    /// Sets the timeout duration after which a peer is disconnected if no keep-alive message is received.
    #[must_use]
    pub fn with_heartbeat_timeout(mut self, timeout: Duration) -> PeerManagerBuilder {
        self.heartbeat_timeout = timeout;
        self
    }

    /// Retrieves the peer capacity.
    #[must_use]
    pub fn peer_capacity(&self) -> usize {
        self.peer_capacity
    }

    /// Retrieves the sink buffer capacity.
    #[must_use]
    pub fn sink_buffer_capacity(&self) -> usize {
        self.sink_buffer_capacity
    }

    /// Retrieves the stream buffer capacity.
    #[must_use]
    pub fn stream_buffer_capacity(&self) -> usize {
        self.stream_buffer_capacity
    }

    /// Retrieves the heartbeat interval `Duration`.
    #[must_use]
    pub fn heartbeat_interval(&self) -> Duration {
        self.heartbeat_interval
    }

    /// Retrieves the heartbeat timeout `Duration`.
    #[must_use]
    pub fn heartbeat_timeout(&self) -> Duration {
        self.heartbeat_timeout
    }

    /// Builds a `PeerManager` from the current `PeerManagerBuilder` configuration.
    #[must_use]
    pub fn build<Peer, Message>(self) -> PeerManager<Peer, Message>
    where
        Peer: Sink<std::io::Result<Message>>
            + Stream<Item = std::io::Result<Message>>
            + TryStream<Ok = Message, Error = std::io::Error>
            + std::fmt::Debug
            + Send
            + Unpin
            + 'static,
        Message: ManagedMessage + Send + 'static,
    {
        PeerManager::from_builder(self)
    }
}
