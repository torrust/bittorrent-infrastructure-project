use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use crossbeam::queue::SegQueue;
use futures::channel::mpsc::{self, SendError};
use futures::sink::Sink;
use futures::task::{Context, Poll};
use futures::{SinkExt as _, Stream, TryStream};

use super::messages::{PeerManagerInputMessage, PeerManagerOutputError, PeerManagerOutputMessage};
use super::task::run_peer;
use crate::manager::builder::PeerManagerBuilder;
use crate::manager::error::PeerManagerError;
use crate::manager::peer_info::PeerInfo;
use crate::manager::ManagedMessage;

/// Sink half of a `PeerManager`.
#[allow(clippy::module_name_repetitions)]
pub struct PeerManagerSink<Peer, Message>
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
    builder: PeerManagerBuilder,
    sender: mpsc::Sender<Result<PeerManagerOutputMessage<Message>, PeerManagerOutputError>>,
    #[allow(clippy::type_complexity)]
    peers: Arc<Mutex<HashMap<PeerInfo, mpsc::Sender<PeerManagerInputMessage<Peer, Message>>>>>,
    task_queue: Arc<SegQueue<tokio::task::JoinHandle<()>>>,
}

impl<Peer, Message> Clone for PeerManagerSink<Peer, Message>
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
    fn clone(&self) -> PeerManagerSink<Peer, Message> {
        PeerManagerSink {
            builder: self.builder,
            sender: self.sender.clone(),
            peers: self.peers.clone(),
            task_queue: self.task_queue.clone(),
        }
    }
}

impl<Peer, Message> PeerManagerSink<Peer, Message>
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
    #[allow(clippy::type_complexity)]
    pub fn new(
        builder: PeerManagerBuilder,
        sender: mpsc::Sender<Result<PeerManagerOutputMessage<Message>, PeerManagerOutputError>>,
        peers: Arc<Mutex<HashMap<PeerInfo, mpsc::Sender<PeerManagerInputMessage<Peer, Message>>>>>,
        task_queue: Arc<SegQueue<tokio::task::JoinHandle<()>>>,
    ) -> PeerManagerSink<Peer, Message> {
        PeerManagerSink {
            builder,
            sender,
            peers,
            task_queue,
        }
    }
}

impl<Peer, Message> PeerManagerSink<Peer, Message>
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
    fn handle_message(
        &self,
        item: std::io::Result<PeerManagerInputMessage<Peer, Message>>,
    ) -> Result<(), PeerManagerError<SendError>> {
        let message = match item {
            Ok(message) => message,
            Err(e) => {
                tracing::debug!("got input message error {e}");
                return Err(PeerManagerError::InputMessageError(e));
            }
        };

        match message {
            PeerManagerInputMessage::AddPeer(info, peer) => self.add_peer(info, peer),
            PeerManagerInputMessage::RemovePeer(info) => self.remove_peer(info),
            PeerManagerInputMessage::SendMessage(info, mid, peer_message) => self.send_message(info, mid, peer_message),
        }
    }

    fn add_peer(&self, info: PeerInfo, peer: Peer) -> Result<(), PeerManagerError<SendError>> {
        tracing::trace!("adding peer: {peer:?}, with info: {info:?}");

        let Ok(mut guard) = self.peers.try_lock() else {
            tracing::debug!("failed to get peers lock");
            return Err(PeerManagerError::LockFailed);
        };

        let cur = guard.len();
        let max = self.builder.peer_capacity();

        if cur >= max {
            tracing::debug!("max peers reached: {cur} of max: {max}");
            return Err(PeerManagerError::PeerStoreFull(guard.len()));
        }

        match guard.entry(info) {
            Entry::Occupied(_) => {
                tracing::debug!("peer already exists: {info:?}");
                return Err(PeerManagerError::PeerAlreadyExists(info));
            }
            Entry::Vacant(vac) => {
                let (sender, task) = run_peer(peer, info, self.sender.clone(), &self.builder);
                vac.insert(sender);
                self.task_queue.push(task); // Add the task to the task queue
            }
        };

        Ok(())
    }

    fn remove_peer(&self, info: PeerInfo) -> Result<(), PeerManagerError<SendError>> {
        tracing::trace!("removing peer, with info: {info:?}");

        let Ok(mut guard) = self.peers.try_lock() else {
            tracing::debug!("failed to get peers lock");
            return Err(PeerManagerError::LockFailed);
        };

        let peer_sender = guard.get_mut(&info).ok_or(PeerManagerError::PeerNotFound(info))?;

        peer_sender
            .start_send(PeerManagerInputMessage::RemovePeer(info))
            .map_err(PeerManagerError::SendFailed)?;

        Ok(())
    }

    fn send_message(&self, info: PeerInfo, mid: u64, msg: Message) -> Result<(), PeerManagerError<SendError>> {
        tracing::trace!("sending message {msg:?}, with info: {info:?}, and mid: {mid}");

        let Ok(mut guard) = self.peers.try_lock() else {
            tracing::debug!("failed to get peers lock");
            return Err(PeerManagerError::LockFailed);
        };

        let peer_sender = guard.get_mut(&info).ok_or(PeerManagerError::PeerNotFound(info))?;

        peer_sender
            .start_send(PeerManagerInputMessage::SendMessage(info, mid, msg))
            .map_err(PeerManagerError::SendFailed)?;

        Ok(())
    }
}

impl<Peer, Message> Sink<std::io::Result<PeerManagerInputMessage<Peer, Message>>> for PeerManagerSink<Peer, Message>
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

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let Ok(mut guard) = self.peers.try_lock() else {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        };

        for peer_sender in guard.values_mut() {
            match peer_sender.poll_ready_unpin(cx) {
                Poll::Ready(Ok(())) => continue,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(PeerManagerError::SendFailed(e))),
                Poll::Pending => return Poll::Pending,
            }
        }

        Poll::Ready(Ok(()))
    }

    fn start_send(
        self: Pin<&mut Self>,
        item: std::io::Result<PeerManagerInputMessage<Peer, Message>>,
    ) -> Result<(), Self::Error> {
        tracing::trace!("handling message: {item:?}");
        self.handle_message(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        tracing::trace!("flushing...");

        let Ok(mut guard) = self.peers.try_lock() else {
            tracing::debug!("failed to get peers lock... will reschedule with waker");
            cx.waker().wake_by_ref();
            return Poll::Pending;
        };

        for peer_sender in guard.values_mut() {
            match peer_sender.poll_flush_unpin(cx) {
                Poll::Ready(Ok(())) => continue,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(PeerManagerError::FlushFailed(e))),
                Poll::Pending => {
                    tracing::debug!("pending to flush peer sender... will reschedule with waker");
                    return Poll::Pending;
                }
            }
        }

        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        tracing::trace!("closing...");

        let Ok(mut guard) = self.peers.try_lock() else {
            tracing::debug!("failed to get peers lock... will reschedule with waker");
            cx.waker().wake_by_ref();
            return Poll::Pending;
        };

        for peer_sender in guard.values_mut() {
            match peer_sender.poll_close_unpin(cx) {
                Poll::Ready(Ok(())) => continue,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(PeerManagerError::FlushFailed(e))),
                Poll::Pending => {
                    tracing::debug!("pending to flush peer sender... will reschedule with waker");
                    return Poll::Pending;
                }
            }
        }

        while let Some(task) = self.task_queue.pop() {
            if !task.is_finished() {
                tracing::debug!("task is not finished... will reschedule with waker");
                self.task_queue.push(task);
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        }

        Poll::Ready(Ok(()))
    }
}
