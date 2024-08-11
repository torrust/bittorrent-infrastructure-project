use std::sync::Once;
use std::time::Duration;

use futures::channel::mpsc::SendError;
use futures::sink::Sink;
use futures::stream::Stream;
use futures::{SinkExt as _, StreamExt as _, TryStream};
use peer::error::PeerManagerError;
use peer::{ManagedMessage, PeerInfo, PeerManagerInputMessage, PeerManagerOutputError, PeerManagerOutputMessage};
use thiserror::Error;
use tokio::time::error::Elapsed;
use tracing::level_filters::LevelFilter;

pub mod connected_channel;

#[derive(Debug, Error)]
pub enum Error<Message>
where
    Message: ManagedMessage + Send + 'static,
{
    #[error("Send Timed Out")]
    SendTimedOut(Elapsed),

    #[error("Receive Timed Out")]
    ReceiveTimedOut(Elapsed),

    #[error("mpsc::Receiver Closed")]
    ReceiverClosed(),

    #[error("Peer Manager Input Error {0}")]
    PeerManagerErr(#[from] PeerManagerError<SendError>),

    #[error("Peer Manager Output Error {0}")]
    PeerManagerOutputErr(#[from] PeerManagerOutputError),

    #[error("Failed to correct response, but got: {0:?}")]
    WrongResponse(#[from] PeerManagerOutputMessage<Message>),

    #[error("Failed to receive Peer Added with matching infohash: got: {0:?}, expected: {1:?}")]
    InfoHashMissMatch(PeerInfo, PeerInfo),
}

#[allow(dead_code)]
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(500);

#[allow(dead_code)]
pub static INIT: Once = Once::new();

#[allow(dead_code)]
pub fn tracing_stderr_init(filter: LevelFilter) {
    let builder = tracing_subscriber::fmt()
        .with_max_level(filter)
        .with_ansi(true)
        .with_writer(std::io::stderr);

    builder.pretty().with_file(true).init();

    tracing::info!("Logging initialized");
}

pub async fn add_peer<Si, St, Peer, Message>(
    send: &mut Si,
    recv: &mut St,
    info: PeerInfo,
    peer: Peer,
) -> Result<(), Error<Message>>
where
    Si: Sink<std::io::Result<PeerManagerInputMessage<Peer, Message>>, Error = PeerManagerError<SendError>> + Unpin,
    St: Stream<Item = Result<PeerManagerOutputMessage<Message>, PeerManagerOutputError>> + Unpin,
    Peer: Sink<std::io::Result<Message>>
        + Stream<Item = std::io::Result<Message>>
        + TryStream<Ok = Message, Error = std::io::Error>
        + std::fmt::Debug
        + Send
        + Unpin
        + 'static,
    Message: ManagedMessage + Send + 'static,
{
    let () = tokio::time::timeout(DEFAULT_TIMEOUT, send.send(Ok(PeerManagerInputMessage::AddPeer(info, peer))))
        .await
        .map_err(|e| Error::SendTimedOut(e))??;

    let response = tokio::time::timeout(DEFAULT_TIMEOUT, recv.next())
        .await
        .map(|res| res.ok_or(Error::ReceiverClosed()))
        .map_err(|e| Error::ReceiveTimedOut(e))???;

    if let PeerManagerOutputMessage::PeerAdded(info_recv) = response {
        if info_recv == info {
            Ok(())
        } else {
            Err(Error::InfoHashMissMatch(info_recv, info))
        }
    } else {
        Err(Error::from(response))
    }
}

pub async fn remove_peer<Si, St, Peer, Message>(send: &mut Si, recv: &mut St, info: PeerInfo) -> Result<(), Error<Message>>
where
    Si: Sink<std::io::Result<PeerManagerInputMessage<Peer, Message>>, Error = PeerManagerError<SendError>> + Unpin,
    St: Stream<Item = Result<PeerManagerOutputMessage<Message>, PeerManagerOutputError>> + Unpin,
    Peer: Sink<std::io::Result<Message>>
        + Stream<Item = std::io::Result<Message>>
        + TryStream<Ok = Message, Error = std::io::Error>
        + std::fmt::Debug
        + Send
        + Unpin
        + 'static,
    Message: ManagedMessage + Send + 'static,
{
    let () = tokio::time::timeout(DEFAULT_TIMEOUT, send.send(Ok(PeerManagerInputMessage::RemovePeer(info))))
        .await
        .map_err(|e| Error::SendTimedOut(e))??;

    let response = tokio::time::timeout(DEFAULT_TIMEOUT, recv.next())
        .await
        .map(|res| res.ok_or(Error::ReceiverClosed()))
        .map_err(|e| Error::ReceiveTimedOut(e))???;

    if let PeerManagerOutputMessage::PeerRemoved(info_recv) = response {
        if info_recv == info {
            Ok(())
        } else {
            Err(Error::InfoHashMissMatch(info_recv, info))
        }
    } else {
        Err(Error::from(response))
    }
}
