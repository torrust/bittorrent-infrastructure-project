use std::ops::{Deref, DerefMut};

use tracing::instrument;

#[allow(clippy::module_name_repetitions)]
pub struct BufferPool {
    // Use Stack For Temporal Locality
    buffers: Vec<Buffer>,
    buffer_size: usize,
}

impl std::fmt::Debug for BufferPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BufferPool")
            .field("buffers_len", &self.buffers.len())
            .field("buffer_size", &self.buffer_size)
            .finish()
    }
}

impl BufferPool {
    #[instrument(skip())]
    pub fn new(buffer_size: usize) -> BufferPool {
        let buffers = Vec::new();

        BufferPool { buffers, buffer_size }
    }

    #[instrument(skip(self), fields(remaining= %self.buffers.len()))]
    pub fn pop(&mut self) -> Buffer {
        if let Some(buffer) = self.buffers.pop() {
            tracing::trace!(?buffer, "popping old buffer taken from pool");
            buffer
        } else {
            let buffer = Buffer::new(self.buffer_size);
            tracing::trace!(?buffer, "creating new buffer...");
            buffer
        }
    }

    #[instrument(skip(self, buffer), fields(existing= %self.buffers.len()))]
    pub fn push(&mut self, mut buffer: Buffer) {
        tracing::trace!("Pushing buffer back to pool");
        buffer.reset_position();
        self.buffers.push(buffer);
    }
}

//----------------------------------------------------------------------------//

/// Reusable region of memory for incoming and outgoing messages.
pub struct Buffer {
    buffer: std::io::Cursor<Vec<u8>>,
}

impl std::fmt::Debug for Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Buffer").field("buffer", &self.as_ref()).finish()
    }
}

impl Buffer {
    #[instrument(skip())]
    fn new(len: usize) -> Buffer {
        Buffer {
            buffer: std::io::Cursor::new(vec![0_u8; len]),
        }
    }

    fn reset_position(&mut self) {
        self.set_position(0);
    }
}

impl Deref for Buffer {
    type Target = std::io::Cursor<Vec<u8>>;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for Buffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

impl AsRef<[u8]> for Buffer {
    fn as_ref(&self) -> &[u8] {
        self.get_ref().split_at(self.buffer.position().try_into().unwrap()).0
    }
}

impl AsMut<[u8]> for Buffer {
    fn as_mut(&mut self) -> &mut [u8] {
        let pos = self.buffer.position().try_into().unwrap();
        self.get_mut().split_at_mut(pos).1
    }
}

#[cfg(test)]
mod tests {

    use super::{Buffer, BufferPool};

    const DEFAULT_BUFFER_SIZE: usize = 1500;

    #[test]
    fn positive_buffer_pool_buffer_len() {
        let mut buffers = BufferPool::new(DEFAULT_BUFFER_SIZE);
        let mut buffer = buffers.pop();

        assert_eq!(buffer.as_mut().len(), DEFAULT_BUFFER_SIZE);
        assert_eq!(buffer.as_ref().len(), 0);
    }

    #[test]
    fn positive_buffer_len_update() {
        let mut buffer = Buffer::new(DEFAULT_BUFFER_SIZE);

        buffer.set_position((DEFAULT_BUFFER_SIZE - 1).try_into().unwrap());

        assert_eq!(buffer.as_mut().len(), 1);
        assert_eq!(buffer.as_ref().len(), DEFAULT_BUFFER_SIZE - 1);
    }
}
