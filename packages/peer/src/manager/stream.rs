use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use futures::channel::mpsc;
use futures::stream::Stream;
use futures::{Sink, StreamExt, TryStream};
use pin_project::pin_project;

use super::messages::{ManagedMessage, PeerManagerInputMessage, PeerManagerOutputError, PeerManagerOutputMessage};
use crate::manager::peer_info::PeerInfo;

/// Stream half of a `PeerManager`.
#[allow(clippy::module_name_repetitions)]
#[pin_project]
pub struct PeerManagerStream<Peer, Message>
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
    recv: mpsc::Receiver<Result<PeerManagerOutputMessage<Message>, PeerManagerOutputError>>,
    #[allow(clippy::type_complexity)]
    peers: Arc<Mutex<HashMap<PeerInfo, mpsc::Sender<PeerManagerInputMessage<Peer, Message>>>>>,
    opt_pending: Option<Result<PeerManagerOutputMessage<Message>, PeerManagerOutputError>>,
}

impl<Peer, Message> PeerManagerStream<Peer, Message>
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
        recv: mpsc::Receiver<Result<PeerManagerOutputMessage<Message>, PeerManagerOutputError>>,
        peers: Arc<Mutex<HashMap<PeerInfo, mpsc::Sender<PeerManagerInputMessage<Peer, Message>>>>>,
    ) -> Self {
        Self {
            recv,
            peers,
            opt_pending: None,
        }
    }
}

impl<Peer, Message> Stream for PeerManagerStream<Peer, Message>
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

    fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let next_message = match self.opt_pending.take() {
            Some(message) => message,
            None => match self.recv.poll_next_unpin(cx) {
                Poll::Ready(Some(message)) => message,
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            },
        };

        let ready = match next_message {
            Err(err) => match err {
                PeerManagerOutputError::PeerError(info, _) => {
                    let Ok(mut peers) = self.peers.try_lock() else {
                        cx.waker().wake_by_ref();
                        return Poll::Pending;
                    };

                    match peers.remove(&info) {
                        Some(peer) => {
                            drop(peer);
                            Poll::Ready(Some(Ok(PeerManagerOutputMessage::PeerRemoved(info))))
                        }
                        None => Poll::Ready(Some(Err(PeerManagerOutputError::PeerErrorAndMissing(
                            info,
                            Some(Box::new(err)),
                        )))),
                    }
                }
                PeerManagerOutputError::PeerErrorAndMissing(_, _)
                | PeerManagerOutputError::PeerDisconnectedAndMissing(_)
                | PeerManagerOutputError::PeerRemovedAndMissing(_) => Poll::Ready(Some(Err(err))),
            },
            Ok(PeerManagerOutputMessage::PeerRemoved(info)) => {
                let Ok(mut peers) = self.peers.try_lock() else {
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                };

                match peers.remove(&info) {
                    Some(peer) => {
                        drop(peer);
                        Poll::Ready(Some(Ok(PeerManagerOutputMessage::PeerRemoved(info))))
                    }
                    None => Poll::Ready(Some(Err(PeerManagerOutputError::PeerRemovedAndMissing(info)))),
                }
            }

            Ok(PeerManagerOutputMessage::PeerDisconnect(info)) => {
                let Ok(mut peers) = self.peers.try_lock() else {
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                };

                match peers.remove(&info) {
                    Some(peer) => {
                        drop(peer);
                        Poll::Ready(Some(Ok(PeerManagerOutputMessage::PeerRemoved(info))))
                    }
                    None => Poll::Ready(Some(Err(PeerManagerOutputError::PeerDisconnectedAndMissing(info)))),
                }
            }

            Ok(msg) => Poll::Ready(Some(Ok(msg))),
        };

        ready
    }
}
