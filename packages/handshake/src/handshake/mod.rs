use std::task::{Context, Poll};

use builder::HandshakerBuilder;
use futures::channel::mpsc;
use futures::{Sink, SinkExt as _, Stream, StreamExt as _};
use handler::listener::ListenerHandler;
use handler::{handshaker, initiator};
use sink::HandshakerSink;
use stream::HandshakerStream;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::task::JoinSet;
use util::bt::PeerId;

use crate::filter::filters::Filters;
use crate::local_addr::LocalAddr as _;
use crate::{CompleteMessage, DiscoveryInfo, HandshakeFilter, HandshakeFilters, InitiateMessage, Transport};

pub mod builder;
pub mod config;
pub mod handler;
pub mod sink;
pub mod stream;

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

impl<S> Handshaker<S>
where
    S: AsyncRead + AsyncWrite + std::fmt::Debug + Send + Sync + Unpin + 'static,
{
    async fn with_builder<T>(builder: &HandshakerBuilder, transport: T) -> std::io::Result<(Handshaker<T::Socket>, JoinSet<()>)>
    where
        T: Transport<Socket = S> + Send + Sync + 'static,
        <T as Transport>::Listener: Send,
    {
        let config = builder.config;
        let timeout = std::cmp::max(config.handshake_timeout(), config.connect_timeout());

        let listener = transport.listen(builder.bind, timeout).await?;

        // Resolve our "real" public port
        let open_port = if builder.port == 0 {
            listener.local_addr()?.port()
        } else {
            builder.port
        };

        let (addr_send, addr_recv) = mpsc::channel(config.sink_buffer_size());
        let (hand_send, hand_recv) = mpsc::channel(config.wait_buffer_size());
        let (sock_send, sock_recv) = mpsc::channel(config.done_buffer_size());

        let filters = Filters::new();

        // Hook up our pipeline of handlers which will take some connection info, process it, and forward it

        let mut tasks = JoinSet::new();

        tasks.spawn(handler::loop_handler(
            addr_recv,
            initiator::initiator_handler,
            hand_send.clone(),
            Box::pin((transport, filters.clone(), timeout)),
        ));

        tasks.spawn(handler::loop_handler(
            listener,
            ListenerHandler::new,
            hand_send,
            Box::pin(filters.clone()),
        ));

        tasks.spawn(handler::loop_handler(
            hand_recv,
            handshaker::execute_handshake,
            sock_send,
            Box::pin((builder.ext, builder.pid, filters.clone(), timeout)),
        ));

        let sink = HandshakerSink::new(addr_send, open_port, builder.pid, filters);
        let stream = HandshakerStream::new(sock_recv);

        Ok((Handshaker { sink, stream }, tasks))
    }
}

impl<S> Sink<InitiateMessage> for Handshaker<S> {
    type Error = mpsc::SendError;

    fn poll_ready(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.sink.poll_ready_unpin(cx)
    }

    fn start_send(mut self: std::pin::Pin<&mut Self>, item: InitiateMessage) -> Result<(), Self::Error> {
        self.sink.start_send_unpin(item)
    }

    fn poll_flush(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.sink.poll_flush_unpin(cx)
    }

    fn poll_close(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.sink.poll_close_unpin(cx)
    }
}

impl<S> Stream for Handshaker<S> {
    type Item = std::io::Result<CompleteMessage<S>>;

    fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.stream.poll_next_unpin(cx)
    }
}
