use thiserror::Error;

use crate::manager::peer_info::PeerInfo;

#[allow(clippy::module_name_repetitions)]
#[derive(Error, Debug)]
pub enum PeerManagerError<SendErr> {
    #[error("Input message was an error: {0}")]
    InputMessageError(std::io::Error),

    #[error("Peer Was Not Found With PeerInfo {0:?}")]
    PeerNotFound(PeerInfo),

    #[error("Unable to add new peer to full capacity store. Actual Size: {0} ")]
    PeerStoreFull(usize),

    #[error("Unable to add an already existing Peer {0:?}")]
    PeerAlreadyExists(PeerInfo),

    #[error("Failed to Get Lock For Peer")]
    LockFailed,

    #[error("Failed to Send to Sink")]
    SendFailed(SendErr),

    #[error("Failed to Flush to Sink")]
    FlushFailed(SendErr),

    #[error("Failed to close Sink")]
    CloseFailed(SendErr),
}

pub type PeerManagerResult<T, SendErr> = Result<T, PeerManagerError<SendErr>>;
