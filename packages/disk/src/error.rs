use std::path::PathBuf;

use thiserror::Error;
use util::bt::InfoHash;

#[allow(clippy::module_name_repetitions)]
#[derive(Error, Debug)]
pub enum BlockError {
    #[error("IO error")]
    Io(#[from] std::io::Error),

    #[error("Failed To Load/Process Block Because The InfoHash {hash:?} Is Not Currently Added")]
    InfoHashNotFound { hash: InfoHash },
}

pub type BlockResult<T> = Result<T, BlockError>;

#[allow(clippy::module_name_repetitions)]
#[derive(Error, Debug)]
pub enum TorrentError {
    #[error("Block error")]
    Block(#[from] BlockError),

    #[error("IO error")]
    Io(#[from] std::io::Error),

    #[error("Failed To Add Torrent Because Size Checker Failed For {file_path:?} Where File Size Was {actual_size} But Should Have Been {expected_size}")]
    ExistingFileSizeCheck {
        file_path: PathBuf,
        expected_size: u64,
        actual_size: u64,
    },

    #[error("Failed To Add Torrent Because Another Torrent With The Same InfoHash {hash:?} Is Already Added")]
    ExistingInfoHash { hash: InfoHash },

    #[error("Failed To Remove Torrent Because The InfoHash {hash:?} Is Not Currently Added")]
    InfoHashNotFound { hash: InfoHash },
}

pub type TorrentResult<T> = Result<T, TorrentError>;
