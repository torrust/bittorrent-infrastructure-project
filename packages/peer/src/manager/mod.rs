use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{cmp, io};

use crossbeam::queue::SegQueue;
use futures::sink::Sink;
use futures::stream::Stream;
use futures::sync::mpsc;
use futures::{Poll, StartSend};
use messages::{IPeerManagerMessage, ManagedMessage, OPeerManagerMessage};
use sink::PeerManagerSink;
use stream::PeerManagerStream;
use tokio_core::reactor::Handle;

use crate::manager::builder::PeerManagerBuilder;
use crate::manager::error::PeerManagerError;

pub mod builder;
pub mod error;
pub mod messages;
pub mod peer_info;
pub mod sink;
pub mod stream;

mod fused;
mod task;

// We configure our tick duration based on this, could let users configure this in the future...
const DEFAULT_TIMER_SLOTS: usize = 2048;

/// Manages a set of peers with beating hearts.
#[allow(clippy::module_name_repetitions)]
pub struct PeerManager<P>
where
    P: Sink + Stream,
{
    sink: PeerManagerSink<P>,
    stream: PeerManagerStream<P>,
}

impl<P> PeerManager<P>
where
    P: Sink<SinkError = io::Error> + Stream<Error = io::Error>,
    P::SinkItem: ManagedMessage,
    P::Item: ManagedMessage,
{
    /// Create a new `PeerManager` from the given `PeerManagerBuilder`.
    #[must_use]
    pub fn from_builder(builder: PeerManagerBuilder, handle: Handle) -> PeerManager<P> {
        // We use one timer for manager heartbeat intervals, and one for peer heartbeat timeouts
        let maximum_timers = builder.peer_capacity() * 2;
        let pow_maximum_timers = if maximum_timers & (maximum_timers - 1) == 0 {
            maximum_timers
        } else {
            maximum_timers.next_power_of_two()
        };

        // Figure out the right tick duration to get num slots of 2048.
        // TODO: We could probably let users change this in the future...
        let max_duration = cmp::max(builder.heartbeat_interval(), builder.heartbeat_timeout());
        let tick_duration = Duration::from_millis(max_duration.as_secs() * 1000 / (DEFAULT_TIMER_SLOTS as u64) + 1);
        let timer = tokio_timer::wheel()
            .tick_duration(tick_duration)
            .max_capacity(pow_maximum_timers + 1)
            .channel_capacity(pow_maximum_timers)
            .num_slots(DEFAULT_TIMER_SLOTS)
            .build();

        let (res_send, res_recv) = mpsc::channel(builder.stream_buffer_capacity());
        let peers = Arc::new(Mutex::new(HashMap::new()));
        let task_queue = Arc::new(SegQueue::new());

        let sink = PeerManagerSink::new(handle, timer, builder, res_send, peers.clone(), task_queue.clone());
        let stream = PeerManagerStream::new(res_recv, peers, task_queue);

        PeerManager { sink, stream }
    }

    /// Break the `PeerManager` into a sink and stream.
    ///
    /// The returned sink implements `Clone`.
    pub fn into_parts(self) -> (PeerManagerSink<P>, PeerManagerStream<P>) {
        (self.sink, self.stream)
    }
}

impl<P> Sink for PeerManager<P>
where
    P: Sink<SinkError = io::Error> + Stream<Error = io::Error> + 'static,
    P::SinkItem: ManagedMessage,
    P::Item: ManagedMessage,
{
    type SinkItem = IPeerManagerMessage<P>;
    type SinkError = PeerManagerError;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        self.sink.start_send(item)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.sink.poll_complete()
    }
}

impl<P> Stream for PeerManager<P>
where
    P: Sink + Stream,
{
    type Item = OPeerManagerMessage<P::Item>;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.stream.poll()
    }
}
