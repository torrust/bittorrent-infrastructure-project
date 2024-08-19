use std::collections::VecDeque;
use std::net::SocketAddr;

use mio::{EventLoop, Sender, Timeout, TimerResult};

use crate::buffer::{Buffer, BufferPool};
use crate::dispatcher::{DispatchHandler, Dispatcher};

/// Provides services to dispatcher clients.
pub struct Provider<'a, D: Dispatcher> {
    buffer_pool: &'a mut BufferPool,
    out_queue: &'a mut VecDeque<(Buffer, SocketAddr)>,
    event_loop: &'a mut EventLoop<DispatchHandler<D>>,
}

pub fn new<'a, D: Dispatcher>(
    buffer_pool: &'a mut BufferPool,
    out_queue: &'a mut VecDeque<(Buffer, SocketAddr)>,
    event_loop: &'a mut EventLoop<DispatchHandler<D>>,
) -> Provider<'a, D> {
    Provider {
        buffer_pool,
        out_queue,
        event_loop,
    }
}

impl<'a, D: Dispatcher> Provider<'a, D> {
    /// Grab a channel to send messages to the event loop.
    #[must_use]
    pub fn channel(&self) -> Sender<D::Message> {
        self.event_loop.channel()
    }

    /// Execute a closure with a buffer and send the buffer contents to the
    /// destination address or reclaim the buffer and do not send anything.
    pub fn outgoing<F>(&mut self, out: F)
    where
        F: FnOnce(&mut [u8]) -> Option<(usize, SocketAddr)>,
    {
        let mut buffer = self.buffer_pool.pop();
        let opt_send_to = out(buffer.as_mut());

        match opt_send_to {
            None => self.buffer_pool.push(buffer),
            Some((bytes, addr)) => {
                buffer.set_written(bytes);

                self.out_queue.push_back((buffer, addr));
            }
        }
    }

    /// Set a timeout with the given delay and token.
    ///
    /// # Errors
    ///
    /// It would error when the timeout returns in a error.
    pub fn set_timeout(&mut self, token: D::Timeout, delay: u64) -> TimerResult<Timeout> {
        self.event_loop.timeout_ms(token, delay)
    }

    /// Clear a timeout using the provided timeout identifier.
    pub fn clear_timeout(&mut self, timeout: Timeout) -> bool {
        self.event_loop.clear_timeout(timeout)
    }

    /// Shutdown the event loop.
    pub fn shutdown(&mut self) {
        self.event_loop.shutdown();
    }
}
