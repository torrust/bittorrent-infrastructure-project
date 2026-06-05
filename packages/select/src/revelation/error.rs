//! Module for revelation error types.

use thiserror::Error;
use torrust_handshake::InfoHash;
use torrust_peer::PeerInfo;

#[allow(clippy::module_name_repetitions)]
#[derive(Error, Debug)]
pub enum RevealError {
    #[error("Peer {info:?} Sent An Invalid Message: {message:?}")]
    InvalidMessage { info: PeerInfo, message: String },
    #[error("Metainfo With Hash {hash:?} Has Already Been Added")]
    InvalidMetainfoExists { hash: InfoHash },
    #[error("Metainfo With Hash {hash:?} Was Not Already Added")]
    InvalidMetainfoNotExists { hash: InfoHash },
    #[error("Piece Index {index:?} Was Out Of Range For Hash {hash:?}")]
    InvalidPieceOutOfRange { hash: InfoHash, index: u64 },
}
