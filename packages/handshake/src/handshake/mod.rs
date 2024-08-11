use std::time::Duration;
use std::{cmp, io};

use builder::HandshakerBuilder;
use futures::sink::Sink;
use futures::stream::Stream;
use futures::sync::mpsc::{self, SendError};
use futures::{Poll, StartSend};
use sink::HandshakerSink;
use stream::HandshakerStream;
use tokio_core::reactor::Handle;
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_timer::{self};
use util::bt::PeerId;

use crate::discovery::DiscoveryInfo;
use crate::filter::filters::Filters;
use crate::filter::{HandshakeFilter, HandshakeFilters};
use crate::handshake::handler::listener::ListenerHandler;
use crate::handshake::handler::timer::HandshakeTimer;
use crate::handshake::handler::{handshaker, initiator};
use crate::local_addr::LocalAddr;
use crate::message::complete::CompleteMessage;
use crate::message::initiate::InitiateMessage;
use crate::transport::Transport;

pub mod builder;
pub mod config;
pub mod handler;
pub mod sink;
pub mod stream;

//----------------------------------------------------------------------------------//

/// Handshaker which is both `Stream` and `Sink`.
pub struct Handshaker<S> {
    sink: HandshakerSink,
    stream: HandshakerStream<S>,
}

impl<S> Handshaker<S> {
    /// Splits the `Handshaker` into its parts.
    ///
    /// This is an enhanced version of `Stream::split` in that the returned `Sink` implements
    /// `DiscoveryInfo` so it can be cloned and passed in to different peer discovery services.
    #[must_use]
    pub fn into_parts(self) -> (HandshakerSink, HandshakerStream<S>) {
        (self.sink, self.stream)
    }
}

impl<S> DiscoveryInfo for Handshaker<S> {
    fn port(&self) -> u16 {
        self.sink.port()
    }

    fn peer_id(&self) -> PeerId {
        self.sink.peer_id()
    }
}

impl<S> Handshaker<S>
where
    S: AsyncRead + AsyncWrite + 'static,
{
    fn with_builder<T>(builder: &HandshakerBuilder, transport: T, handle: &Handle) -> io::Result<Handshaker<T::Socket>>
    where
        T: Transport<Socket = S> + 'static,
    {
        let listener = transport.listen(&builder.bind, handle)?;

        // Resolve our "real" public port
        let open_port = if builder.port == 0 {
            listener.local_addr()?.port()
        } else {
            builder.port
        };

        let config = builder.config;
        let (addr_send, addr_recv) = mpsc::channel(config.sink_buffer_size());
        let (hand_send, hand_recv) = mpsc::channel(config.wait_buffer_size());
        let (sock_send, sock_recv) = mpsc::channel(config.done_buffer_size());

        let filters = Filters::new();
        let (handshake_timer, initiate_timer) = configured_handshake_timers(config.handshake_timeout(), config.connect_timeout());

        // Hook up our pipeline of handlers which will take some connection info, process it, and forward it
        handler::loop_handler(
            addr_recv,
            initiator::initiator_handler,
            hand_send.clone(),
            (transport, filters.clone(), handle.clone(), initiate_timer),
            handle,
        );
        handler::loop_handler(listener, ListenerHandler::new, hand_send, filters.clone(), handle);
        handler::loop_handler(
            hand_recv.map(Result::Ok).buffer_unordered(100),
            handshaker::execute_handshake,
            sock_send,
            (builder.ext, builder.pid, filters.clone(), handshake_timer),
            handle,
        );

        let sink = HandshakerSink::new(addr_send, open_port, builder.pid, filters);
        let stream = HandshakerStream::new(sock_recv);

        Ok(Handshaker { sink, stream })
    }
}

/// Configure a timer wheel and create a `HandshakeTimer`.
fn configured_handshake_timers(duration_one: Duration, duration_two: Duration) -> (HandshakeTimer, HandshakeTimer) {
    let timer = tokio_timer::wheel()
        .num_slots(64)
        .max_timeout(cmp::max(duration_one, duration_two))
        .build();

    (
        HandshakeTimer::new(timer.clone(), duration_one),
        HandshakeTimer::new(timer, duration_two),
    )
}

impl<S> Sink for Handshaker<S> {
    type SinkItem = InitiateMessage;
    type SinkError = SendError<InitiateMessage>;

    fn start_send(&mut self, item: InitiateMessage) -> StartSend<InitiateMessage, SendError<InitiateMessage>> {
        self.sink.start_send(item)
    }

    fn poll_complete(&mut self) -> Poll<(), SendError<InitiateMessage>> {
        self.sink.poll_complete()
    }
}

impl<S> Stream for Handshaker<S> {
    type Item = CompleteMessage<S>;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<CompleteMessage<S>>, ()> {
        self.stream.poll()
    }
}

impl<S> HandshakeFilters for Handshaker<S> {
    fn add_filter<F>(&self, filter: F)
    where
        F: HandshakeFilter + PartialEq + Eq + Send + Sync + 'static,
    {
        self.sink.add_filter(filter);
    }

    fn remove_filter<F>(&self, filter: F)
    where
        F: HandshakeFilter + PartialEq + Eq + Send + Sync + 'static,
    {
        self.sink.remove_filter(filter);
    }

    fn clear_filters(&self) {
        self.sink.clear_filters();
    }
}
