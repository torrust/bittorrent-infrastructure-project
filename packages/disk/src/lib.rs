mod disk;
mod memory;

/// Both `Block` and `Torrent` error types.
pub mod error;

pub use crate::disk::fs::FileSystem;
pub use crate::disk::manager::builder::DiskManagerBuilder;
pub use crate::disk::manager::{DiskManager, DiskManagerSink, DiskManagerStream};
pub use crate::disk::{IDiskMessage, ODiskMessage};
pub use crate::memory::block::{Block, BlockMetadata, BlockMut};

/// Built in objects implementing `FileSystem`.
pub mod fs {
    pub use crate::disk::fs::native::{NativeFile, NativeFileSystem};
}

/// Built in objects implementing `FileSystem` for caching.
pub mod fs_cache {
    pub use crate::disk::fs::cache::file_handle::FileHandleCache;
}

pub use util::bt::InfoHash;
