use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

use crossbeam::queue::SegQueue;
use error::PeerManagerError;
use futures::channel::mpsc::{self, SendError};
use futures::sink::Sink;
use futures::stream::Stream;
use futures::{SinkExt as _, StreamExt, TryStream};
use sink::PeerManagerSink;

use super::ManagedMessage;
use crate::{PeerManagerBuilder, PeerManagerInputMessage, PeerManagerOutputError, PeerManagerOutputMessage, PeerManagerStream};

pub mod builder;
pub mod error;
pub mod messages;
pub mod peer_info;
pub mod sink;
pub mod stream;

mod fused;
mod task;

/// Manages a set of peers with beating hearts.
#[allow(clippy::module_name_repetitions)]
pub struct PeerManager<Peer, Message>
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
    sink: PeerManagerSink<Peer, Message>,
    stream: PeerManagerStream<Peer, Message>,
    _peer_marker: PhantomData<Peer>,
}

impl<Peer, Message> PeerManager<Peer, Message>
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
    /// Create a new `PeerManager` from the given `PeerManagerBuilder`.
    #[must_use]
    pub fn from_builder(builder: PeerManagerBuilder) -> PeerManager<Peer, Message> {
        let (res_send, res_recv) = mpsc::channel(builder.stream_buffer_capacity());
        let peers = Arc::new(Mutex::new(HashMap::new()));
        let task_queue = Arc::new(SegQueue::new());

        let sink = PeerManagerSink::new(builder, res_send, peers.clone(), task_queue.clone());
        let stream = PeerManagerStream::new(res_recv, peers);

        PeerManager {
            sink,
            stream,
            _peer_marker: PhantomData,
        }
    }

    /// Break the `PeerManager` into a sink and stream.
    ///
    /// The returned sink implements `Clone`.
    pub fn into_parts(self) -> (PeerManagerSink<Peer, Message>, PeerManagerStream<Peer, Message>) {
        (self.sink, self.stream)
    }
}

impl<Peer, Message> Sink<std::io::Result<PeerManagerInputMessage<Peer, Message>>> for PeerManager<Peer, Message>
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
    type Error = PeerManagerError<SendError>;

    fn poll_ready(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.sink.poll_ready_unpin(cx)
    }

    fn start_send(
        mut self: std::pin::Pin<&mut Self>,
        item: std::io::Result<PeerManagerInputMessage<Peer, Message>>,
    ) -> Result<(), Self::Error> {
        self.sink.start_send_unpin(item)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.sink.poll_flush_unpin(cx)
    }

    fn poll_close(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.sink.poll_close_unpin(cx)
    }
}

impl<Peer, Message> Stream for PeerManager<Peer, Message>
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
    type Item = Result<PeerManagerOutputMessage<Message>, PeerManagerOutputError>;

    fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
        self.stream.poll_next_unpin(cx)
    }
}
