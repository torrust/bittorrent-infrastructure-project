use std::sync::Arc;

use crate::disk::fs::FileSystem;
use crate::disk::manager::DiskManager;

const DEFAULT_PENDING_SIZE: usize = 10;
const DEFAULT_COMPLETED_SIZE: usize = 10;
const DEFAULT_THREAD_POOL_SIZE: usize = 4;

/// `DiskManagerBuilder` for building `DiskManager`s with different settings.
#[allow(clippy::module_name_repetitions)]
pub struct DiskManagerBuilder {
    thread_pool_size: usize,
    pending_size: usize,
    completed_size: usize,
}

impl Default for DiskManagerBuilder {
    fn default() -> Self {
        Self {
            thread_pool_size: DEFAULT_THREAD_POOL_SIZE,
            pending_size: DEFAULT_PENDING_SIZE,
            completed_size: DEFAULT_COMPLETED_SIZE,
        }
    }
}

impl DiskManagerBuilder {
    /// Create a new `DiskManagerBuilder`.
    #[must_use]
    pub fn new() -> DiskManagerBuilder {
        DiskManagerBuilder::default()
    }

    /// Specify the number of threads for the `ThreadPool`.
    #[must_use]
    pub fn with_thread_pool_size(mut self, size: usize) -> DiskManagerBuilder {
        self.thread_pool_size = size;
        self
    }

    /// Specify the buffer capacity for pending `IDiskMessage`s.
    #[must_use]
    pub fn with_sink_buffer_capacity(mut self, size: usize) -> DiskManagerBuilder {
        self.pending_size = size;
        self
    }

    /// Specify the buffer capacity for completed `ODiskMessage`s.
    #[must_use]
    pub fn with_stream_buffer_capacity(mut self, size: usize) -> DiskManagerBuilder {
        self.completed_size = size;
        self
    }

    /// Retrieve the `ThreadPool` size.
    #[must_use]
    pub fn thread_pool_size(&self) -> usize {
        self.thread_pool_size
    }

    /// Retrieve the sink buffer capacity.
    #[must_use]
    pub fn sink_buffer_capacity(&self) -> usize {
        self.pending_size
    }

    /// Retrieve the stream buffer capacity.
    #[must_use]
    pub fn stream_buffer_capacity(&self) -> usize {
        self.completed_size
    }

    /// Build a `DiskManager` with the given `FileSystem`.
    pub fn build<F>(self, fs: Arc<F>) -> DiskManager<F>
    where
        F: FileSystem + Sync + 'static,
        Arc<F>: Send + Sync,
    {
        DiskManager::from_builder(&self, fs)
    }
}
