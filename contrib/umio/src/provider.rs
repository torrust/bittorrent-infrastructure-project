use std::collections::VecDeque;
use std::io::Write;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::sync::mpsc;
use std::time::Instant;

use mio::Waker;
use tracing::instrument;

use crate::buffer::{Buffer, BufferPool};
use crate::dispatcher::Dispatcher;
use crate::eloop::ShutdownHandle;

pub enum TimeoutAction<T>
where
    T: std::fmt::Debug,
{
    Add { token: T, when: Instant },
    Remove { token: T },
}

#[derive(Debug)]
pub struct Provider<'a, D>
where
    D: Dispatcher + std::fmt::Debug,
{
    buffer_pool: &'a mut BufferPool,
    buffer: Option<Buffer>,
    out_queue: &'a mut VecDeque<(Buffer, SocketAddr)>,
    waker: &'a Waker,
    shutdown_handle: &'a mut ShutdownHandle,
    timer_sender: &'a mpsc::Sender<TimeoutAction<D::TimeoutToken>>,
    outgoing_socket: Option<SocketAddr>,
    _marker: PhantomData<D>,
}

impl<'a, D> Write for Provider<'a, D>
where
    D: Dispatcher + std::fmt::Debug,
{
    #[instrument(skip(self), fields(buffer= ?self.buffer))]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let dest = self.buffer.get_or_insert_with(|| self.buffer_pool.pop());

        let wrote = dest.write(buf)?;

        tracing::trace!(%wrote, "write");

        Ok(wrote)
    }

    #[instrument(skip(self))]
    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(buffer) = self.buffer.take() {
            tracing::trace!(?buffer, "flushing...");

            if let Some(addr) = self.outgoing_socket {
                self.out_queue.push_back((buffer, addr));
                self.wake();
            } else {
                self.buffer_pool.push(buffer);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "No outgoing socket address set",
                ));
            }
        } else {
            tracing::warn!("flush empty");
        }
        Ok(())
    }
}

impl<'a, D> Provider<'a, D>
where
    D: Dispatcher + std::fmt::Debug,
{
    #[instrument(skip())]
    pub fn new(
        buffer_pool: &'a mut BufferPool,
        out_queue: &'a mut VecDeque<(Buffer, SocketAddr)>,
        waker: &'a Waker,
        shutdown_handle: &'a mut ShutdownHandle,
        timer_sender: &'a mpsc::Sender<TimeoutAction<D::TimeoutToken>>,
    ) -> Provider<'a, D> {
        Provider {
            buffer_pool,
            buffer: None,
            out_queue,
            waker,
            timer_sender,
            shutdown_handle,
            outgoing_socket: None,
            _marker: PhantomData,
        }
    }

    #[instrument(skip(self))]
    pub fn set_dest(&mut self, dest: SocketAddr) -> Option<SocketAddr> {
        self.outgoing_socket.replace(dest)
    }

    /// Wakes the event loop.
    ///
    /// # Panics
    ///
    /// This function will panic if waking the event loop fails.
    #[instrument(skip(self))]
    pub fn wake(&self) {
        self.waker.wake().expect("Failed to wake the event loop");
    }

    /// Sets a timeout with the given token and delay.
    ///
    /// # Errors
    ///
    /// This function will return an error if sending message fails.
    #[instrument(skip(self, token, when))]
    pub fn set_timeout(&mut self, token: D::TimeoutToken, when: Instant) -> Result<(), Box<dyn std::error::Error>>
    where
        D::TimeoutToken: 'static,
    {
        tracing::trace!(?token, ?when, "set timeout");

        self.timer_sender.send(TimeoutAction::Add { token, when })?;
        self.wake();
        Ok(())
    }

    /// Removes a timeout
    ///
    /// # Errors
    ///
    /// This function will return an error if sending message fails.
    #[instrument(skip(self))]
    pub fn remove_timeout(&mut self, token: D::TimeoutToken) -> Result<(), Box<dyn std::error::Error>>
    where
        D::TimeoutToken: 'static,
    {
        tracing::trace!("remove timeout");

        self.timer_sender.send(TimeoutAction::Remove { token })?;
        self.wake();
        Ok(())
    }

    /// Shuts down the event loop.
    ///
    /// # Panics
    ///
    /// This function will panic if sending the shutdown signal fails.
    #[instrument(skip(self))]
    pub fn shutdown(&mut self) {
        tracing::debug!("shutdown");

        self.shutdown_handle.shutdown();
    }
}
