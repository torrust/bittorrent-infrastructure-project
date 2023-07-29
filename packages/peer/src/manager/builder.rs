use std::io;
use std::time::Duration;

use futures::sink::Sink;
use futures::stream::Stream;
use tokio_core::reactor::Handle;

use crate::manager::{ManagedMessage, PeerManager};

const DEFAULT_PEER_CAPACITY: usize = 1000;
const DEFAULT_SINK_BUFFER_CAPACITY: usize = 100;
const DEFAULT_STREAM_BUFFER_CAPACITY: usize = 100;
const DEFAULT_HEARTBEAT_INTERVAL_MILLIS: u64 = 60 * 1000;
const DEFAULT_HEARTBEAT_TIMEOUT_MILLIS: u64 = 2 * 60 * 1000;

/// Builder for configuring a `PeerManager`.
#[derive(Copy, Clone)]
pub struct PeerManagerBuilder {
    peer: usize,
    sink_buffer: usize,
    stream_buffer: usize,
    heartbeat_interval: Duration,
    heartbeat_timeout: Duration,
}

impl PeerManagerBuilder {
    /// Create a new `PeerManagerBuilder`.
    #[must_use]
    pub fn new() -> PeerManagerBuilder {
        PeerManagerBuilder {
            peer: DEFAULT_PEER_CAPACITY,
            sink_buffer: DEFAULT_SINK_BUFFER_CAPACITY,
            stream_buffer: DEFAULT_STREAM_BUFFER_CAPACITY,
            heartbeat_interval: Duration::from_millis(DEFAULT_HEARTBEAT_INTERVAL_MILLIS),
            heartbeat_timeout: Duration::from_millis(DEFAULT_HEARTBEAT_TIMEOUT_MILLIS),
        }
    }

    /// Max number of peers we can manage.
    #[must_use]
    pub fn with_peer_capacity(mut self, capacity: usize) -> PeerManagerBuilder {
        self.peer = capacity;
        self
    }

    /// Capacity of pending sent messages.
    #[must_use]
    pub fn with_sink_buffer_capacity(mut self, capacity: usize) -> PeerManagerBuilder {
        self.sink_buffer = capacity;
        self
    }

    /// Capacity of pending received messages.
    #[must_use]
    pub fn with_stream_buffer_capacity(mut self, capacity: usize) -> PeerManagerBuilder {
        self.stream_buffer = capacity;
        self
    }

    /// Interval at which we send keep-alive messages.
    #[must_use]
    pub fn with_heartbeat_interval(mut self, interval: Duration) -> PeerManagerBuilder {
        self.heartbeat_interval = interval;
        self
    }

    /// Timeout at which we disconnect from the peer without seeing a keep-alive message.
    #[must_use]
    pub fn with_heartbeat_timeout(mut self, timeout: Duration) -> PeerManagerBuilder {
        self.heartbeat_timeout = timeout;
        self
    }

    /// Retrieve the peer capacity.
    #[must_use]
    pub fn peer_capacity(&self) -> usize {
        self.peer
    }

    /// Retrieve the sink buffer capacity.
    #[must_use]
    pub fn sink_buffer_capacity(&self) -> usize {
        self.sink_buffer
    }

    /// Retrieve the stream buffer capacity.
    #[must_use]
    pub fn stream_buffer_capacity(&self) -> usize {
        self.stream_buffer
    }

    /// Retrieve the heartbeat interval `Duration`.
    #[must_use]
    pub fn heartbeat_interval(&self) -> Duration {
        self.heartbeat_interval
    }

    /// Retrieve the heartbeat timeout `Duration`.
    #[must_use]
    pub fn heartbeat_timeout(&self) -> Duration {
        self.heartbeat_timeout
    }

    /// Build a `PeerManager` from the current `PeerManagerBuilder`.
    #[must_use]
    pub fn build<P>(self, handle: Handle) -> PeerManager<P>
    where
        P: Sink<SinkError = io::Error> + Stream<Error = io::Error>,
        P::SinkItem: ManagedMessage,
        P::Item: ManagedMessage,
    {
        PeerManager::from_builder(self, handle)
    }
}
