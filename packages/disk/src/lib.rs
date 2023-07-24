extern crate bytes;
extern crate crossbeam;
extern crate metainfo;
extern crate util;
#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate futures_cpupool;
#[macro_use]
extern crate log;
extern crate lru_cache;

mod disk;
mod memory;

/// Both `Block` and `Torrent` error types.
pub mod error;

pub use disk::builder::DiskManagerBuilder;
pub use disk::fs::FileSystem;
pub use disk::manager::{DiskManager, DiskManagerSink, DiskManagerStream};
pub use disk::{IDiskMessage, ODiskMessage};
pub use memory::block::{Block, BlockMetadata, BlockMut};

/// Built in objects implementing `FileSystem`.
pub mod fs {
    pub use crate::disk::fs::native::{NativeFile, NativeFileSystem};
}

/// Built in objects implementing `FileSystem` for caching.
pub mod fs_cache {
    pub use crate::disk::fs::cache::file_handle::FileHandleCache;
}

pub use util::bt::InfoHash;
