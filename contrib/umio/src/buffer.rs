#[allow(clippy::module_name_repetitions)]
pub struct BufferPool {
    // Use Stack For Temporal Locality
    buffers: Vec<Buffer>,
    buffer_size: usize,
}

impl BufferPool {
    pub fn new(buffer_size: usize) -> BufferPool {
        let buffers = Vec::new();

        BufferPool { buffers, buffer_size }
    }

    pub fn pop(&mut self) -> Buffer {
        self.buffers.pop().unwrap_or(Buffer::new(self.buffer_size))
    }

    pub fn push(&mut self, mut buffer: Buffer) {
        buffer.reset_position();

        self.buffers.push(buffer);
    }
}

//----------------------------------------------------------------------------//

/// Reusable region of memory for incoming and outgoing messages.
pub struct Buffer {
    buffer: Vec<u8>,
    bytes_written: usize,
}

impl Buffer {
    fn new(len: usize) -> Buffer {
        Buffer {
            buffer: vec![0u8; len],
            bytes_written: 0,
        }
    }

    fn reset_position(&mut self) {
        self.set_written(0);
    }

    /// Update the number of bytes written to the buffer.
    pub fn set_written(&mut self, bytes: usize) {
        self.bytes_written = bytes;
    }
}

impl AsRef<[u8]> for Buffer {
    fn as_ref(&self) -> &[u8] {
        &self.buffer[..self.bytes_written]
    }
}

impl AsMut<[u8]> for Buffer {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.buffer[self.bytes_written..]
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
        buffer.set_written(DEFAULT_BUFFER_SIZE - 1);

        assert_eq!(buffer.as_mut().len(), 1);
        assert_eq!(buffer.as_ref().len(), DEFAULT_BUFFER_SIZE - 1);
    }
}
