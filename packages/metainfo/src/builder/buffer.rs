use std::time::Duration;

use crossbeam::queue::SegQueue;

// Ensures that we have enough buffers to keep workers busy.
const TOTAL_BUFFERS_MULTIPLICATIVE: usize = 2;
const TOTAL_BUFFERS_ADDITIVE: usize = 0;

/// Stores a set number of piece buffers to be used and re-used.
pub struct PieceBuffers {
    piece_queue: SegQueue<PieceBuffer>,
}

impl PieceBuffers {
    /// Create a new queue filled with a number of piece buffers based on the number of workers.
    pub fn new(piece_length: usize, num_workers: usize) -> PieceBuffers {
        let piece_queue = SegQueue::new();

        let total_buffers = calculate_total_buffers(num_workers);
        for _ in 0..total_buffers {
            piece_queue.push(PieceBuffer::new(piece_length));
        }

        PieceBuffers { piece_queue }
    }

    /// Checkin the given piece buffer to be re-used.
    pub fn checkin(&self, mut buffer: PieceBuffer) {
        buffer.bytes_read = 0;

        self.piece_queue.push(buffer);
    }

    /// Checkout a piece buffer (possibly blocking) to be used.
    pub fn checkout(&self) -> PieceBuffer {
        let mut pb = None;
        let ten_millis = Duration::from_millis(10);

        while pb.is_none() {
            pb = self.piece_queue.pop();

            if pb.is_some() {
                break;
            }

            std::thread::sleep(ten_millis);
            continue;
        }

        pb.expect("Checked is_some in loop above.")
    }
}

/// Calculates the optimal number of piece buffers given the number of workers.
fn calculate_total_buffers(num_workers: usize) -> usize {
    num_workers * TOTAL_BUFFERS_MULTIPLICATIVE + TOTAL_BUFFERS_ADDITIVE
}

// ----------------------------------------------------------------------------//

/// Piece buffer that can be filled up until it contains a full piece.
#[allow(clippy::module_name_repetitions)]
#[derive(PartialEq, Eq)]
pub struct PieceBuffer {
    buffer: Vec<u8>,
    bytes_read: usize,
}

impl PieceBuffer {
    /// Create a new piece buffer.
    fn new(piece_length: usize) -> PieceBuffer {
        PieceBuffer {
            buffer: vec![0u8; piece_length],
            bytes_read: 0,
        }
    }

    pub fn write_bytes<C>(&mut self, mut callback: C) -> std::io::Result<usize>
    where
        C: FnMut(&mut [u8]) -> std::io::Result<usize>,
    {
        let new_bytes_read = callback(&mut self.buffer[self.bytes_read..])?;
        self.bytes_read += new_bytes_read;

        Ok(new_bytes_read)
    }

    /// Whether or not the given piece buffer is full.
    pub fn is_whole(&self) -> bool {
        self.bytes_read == self.buffer.len()
    }

    /// Whether or not the given piece buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes_read == 0
    }

    /// Access the piece buffer as a byte slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.buffer[..self.bytes_read]
    }
}
