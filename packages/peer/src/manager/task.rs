use futures::channel::mpsc::{self, SendError};
use futures::stream::SplitSink;
use futures::{Sink, SinkExt, Stream, StreamExt, TryStream, TryStreamExt};
use thiserror::Error;
use tokio::task::{self, JoinHandle};

use super::fused::{PersistentError, PersistentStream, RecurringTimeoutError, RecurringTimeoutStream};
use super::messages::{PeerManagerInputMessage, PeerManagerOutputMessage};
use crate::manager::builder::PeerManagerBuilder;
use crate::manager::peer_info::PeerInfo;
use crate::manager::ManagedMessage;
use crate::PeerManagerOutputError;

#[derive(Error, Debug)]
enum PeerError<PeerSendErr, ManagerSendErr> {
    #[error("Manager Error")]
    ManagerDisconnect(ManagerSendErr),
    #[error("Stream Finished")]
    Disconnected,
    #[error("Peer Error")]
    PeerDisconnect(PeerSendErr),
    #[error("Peer Removed")]
    PeerRemoved(PeerInfo),
}

enum UnifiedError<Err> {
    Peer(PersistentError<Err>),
    Manager(RecurringTimeoutError<Err>),
}

enum MergedError<Err> {
    Disconnect,
    StreamError(Err),
    Timeout,
}

impl<Err> From<UnifiedError<Err>> for MergedError<Err> {
    fn from(err: UnifiedError<Err>) -> Self {
        match err {
            UnifiedError::Peer(PersistentError::Disconnect) | UnifiedError::Manager(RecurringTimeoutError::Disconnect) => {
                Self::Disconnect
            }
            UnifiedError::Peer(PersistentError::StreamError(err))
            | UnifiedError::Manager(RecurringTimeoutError::StreamError(err)) => Self::StreamError(err),
            UnifiedError::Manager(RecurringTimeoutError::Timeout) => Self::Timeout,
        }
    }
}

enum UnifiedItem<Peer, Message>
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
    Peer(Message),
    Manager(PeerManagerInputMessage<Peer, Message>),
}

pub fn run_peer<Peer, Message>(
    peer: Peer,
    info: PeerInfo,
    mut send: mpsc::Sender<Result<PeerManagerOutputMessage<Message>, PeerManagerOutputError>>,
    builder: &PeerManagerBuilder,
) -> (mpsc::Sender<PeerManagerInputMessage<Peer, Message>>, JoinHandle<()>)
where
    Peer: Sink<std::io::Result<Message>>
        + Stream<Item = std::io::Result<Message>>
        + TryStream<Ok = Message, Error = std::io::Error>
        + std::fmt::Debug
        + Send
        + StreamExt
        + Unpin
        + 'static,
    Message: ManagedMessage + Send + 'static,
{
    let (manager_send, manager_recv) = mpsc::channel(builder.sink_buffer_capacity());
    let (mut peer_send, peer_recv) = peer.split();

    let heartbeat_interval = builder.heartbeat_interval();

    let peer_stream = Box::pin(
        PersistentStream::new(peer_recv)
            .map_err(UnifiedError::Peer)
            .map_ok(|i| UnifiedItem::Peer(i)),
    );

    let manager_stream = Box::pin(
        RecurringTimeoutStream::new(manager_recv.map(Ok), heartbeat_interval)
            .map_err(UnifiedError::Manager)
            .map_ok(|i| UnifiedItem::Manager(i)),
    );

    let mut merged_stream = Box::pin(futures::stream::select(peer_stream, manager_stream).map_err(MergedError::from));

    let task = task::spawn(async move {
        if send.send(Ok(PeerManagerOutputMessage::PeerAdded(info))).await.is_err() {
            return;
        }

        while let Some(result) = merged_stream.as_mut().next().await {
            if handle_stream_result::<Peer, Message>(result, &mut peer_send, &mut send, &info)
                .await
                .is_err()
            {
                break;
            }
        }
    });

    (manager_send, task)
}

async fn handle_stream_result<Peer, Message>(
    result: Result<UnifiedItem<Peer, Message>, MergedError<std::io::Error>>,
    peer_send: &mut SplitSink<Peer, std::io::Result<Message>>,
    manager_send: &mut mpsc::Sender<Result<PeerManagerOutputMessage<Message>, PeerManagerOutputError>>,
    info: &PeerInfo,
) -> Result<(), PeerError<<Peer as Sink<std::io::Result<Message>>>::Error, SendError>>
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
    match result {
        Ok(UnifiedItem::Peer(message)) => {
            // Handle peer message
            manager_send
                .send(Ok(PeerManagerOutputMessage::ReceivedMessage(*info, message)))
                .await
                .map_err(PeerError::ManagerDisconnect)?;

            Ok(())
        }
        Ok(UnifiedItem::Manager(PeerManagerInputMessage::AddPeer(_, _))) => panic!("invalid message"),
        Ok(UnifiedItem::Manager(PeerManagerInputMessage::RemovePeer(info))) => {
            manager_send
                .send(Ok(PeerManagerOutputMessage::PeerRemoved(info)))
                .await
                .map_err(PeerError::ManagerDisconnect)?;

            Err(PeerError::PeerRemoved(info))
        }
        Ok(UnifiedItem::Manager(PeerManagerInputMessage::SendMessage(info, id, message))) => {
            peer_send.send(Ok(message)).await.map_err(PeerError::PeerDisconnect)?;
            manager_send
                .send(Ok(PeerManagerOutputMessage::SentMessage(info, id)))
                .await
                .map_err(PeerError::ManagerDisconnect)?;

            Ok(())
        }
        Err(MergedError::Disconnect) => Err(PeerError::Disconnected),
        Err(MergedError::StreamError(e)) => {
            // Handle stream error
            manager_send
                .send(Err(PeerManagerOutputError::PeerError(*info, e)))
                .await
                .map_err(PeerError::ManagerDisconnect)?;

            Err(PeerError::PeerRemoved(*info))
        }
        Err(MergedError::Timeout) => {
            peer_send
                .send(Ok(Message::keep_alive()))
                .await
                .map_err(PeerError::PeerDisconnect)?;

            Ok(())
        }
    }
}
