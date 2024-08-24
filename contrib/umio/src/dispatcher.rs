use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::mpsc;

use mio::net::UdpSocket;
use mio::{Interest, Poll, Waker};
use tracing::{instrument, Level};

use crate::buffer::{Buffer, BufferPool};
use crate::eloop::ShutdownHandle;
use crate::provider::TimeoutAction;
use crate::{Provider, UDP_SOCKET_TOKEN};

pub trait Dispatcher: Sized + std::fmt::Debug {
    type TimeoutToken: std::fmt::Debug;
    type Message: std::fmt::Debug;

    fn incoming(&mut self, _provider: Provider<'_, Self>, _message: &[u8], _addr: SocketAddr) {}
    fn notify(&mut self, _provider: Provider<'_, Self>, _message: Self::Message) {}
    fn timeout(&mut self, _provider: Provider<'_, Self>, _timeout: Self::TimeoutToken) {}
}

pub struct DispatchHandler<D: Dispatcher>
where
    D: std::fmt::Debug,
{
    pub dispatch: D,
    pub out_queue: VecDeque<(Buffer, SocketAddr)>,
    socket: UdpSocket,
    pub buffer_pool: BufferPool,
    current_interest: Interest,
    pub timer_sender: mpsc::Sender<TimeoutAction<D::TimeoutToken>>,
}

impl<D: Dispatcher + std::fmt::Debug> std::fmt::Debug for DispatchHandler<D>
where
    D: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DispatchHandler")
            .field("dispatch", &self.dispatch)
            .field("out_queue_len", &self.out_queue.len())
            .field("socket", &self.socket)
            .field("buffer_pool", &self.buffer_pool)
            .field("current_interest", &self.current_interest)
            .field("timer_sender", &self.timer_sender)
            .finish()
    }
}

impl<D> DispatchHandler<D>
where
    D: Dispatcher + std::fmt::Debug,
{
    #[instrument(skip(), ret(level = Level::TRACE))]
    pub fn new(
        mut socket: UdpSocket,
        buffer_size: usize,
        dispatch: D,
        poll: &mut Poll,
        timer_sender: mpsc::Sender<TimeoutAction<D::TimeoutToken>>,
    ) -> DispatchHandler<D>
    where
        D: std::fmt::Debug,
        <D as Dispatcher>::TimeoutToken: std::fmt::Debug,
        <D as Dispatcher>::Message: std::fmt::Debug,
    {
        let buffer_pool = BufferPool::new(buffer_size);
        let out_queue = VecDeque::new();

        poll.registry()
            .register(&mut socket, UDP_SOCKET_TOKEN, Interest::READABLE)
            .unwrap();

        DispatchHandler {
            dispatch,
            out_queue,
            socket,
            buffer_pool,
            current_interest: Interest::READABLE,
            timer_sender,
        }
    }

    #[instrument(skip(self, waker, shutdown_handle))]
    pub fn handle_message(&mut self, waker: &Waker, shutdown_handle: &mut ShutdownHandle, message: D::Message) {
        tracing::trace!("message received");
        let provider = Provider::new(
            &mut self.buffer_pool,
            &mut self.out_queue,
            waker,
            shutdown_handle,
            &self.timer_sender,
        );

        self.dispatch.notify(provider, message);
    }

    #[instrument(skip(self, waker, shutdown_handle))]
    pub fn handle_timeout(&mut self, waker: &Waker, shutdown_handle: &mut ShutdownHandle, token: D::TimeoutToken) {
        tracing::trace!("timeout expired");
        let provider = Provider::new(
            &mut self.buffer_pool,
            &mut self.out_queue,
            waker,
            shutdown_handle,
            &self.timer_sender,
        );

        self.dispatch.timeout(provider, token);
    }

    #[instrument(skip(self))]
    pub fn handle_write(&mut self) {
        tracing::trace!("handle write");

        if let Some((buffer, addr)) = self.out_queue.pop_front() {
            let bytes = self.socket.send_to(buffer.as_ref(), addr).unwrap();

            tracing::debug!(?buffer, ?bytes, ?addr, "sent");

            self.buffer_pool.push(buffer);
        }
    }

    #[instrument(skip(self))]
    pub fn handle_read(&mut self) -> Option<(Buffer, SocketAddr)> {
        tracing::trace!("handle read");

        let mut buffer = self.buffer_pool.pop();

        match self.socket.recv_from(buffer.as_mut()) {
            Ok((bytes, addr)) => {
                buffer.set_position(bytes.try_into().unwrap());
                tracing::trace!(?buffer, "DispatchHandler: Read {bytes} bytes from {addr}");

                Some((buffer, addr))
            }
            Err(e) => {
                tracing::error!("DispatchHandler: Failed to read from UDP socket: {e}");
                None
            }
        }
    }

    #[instrument(skip(self, waker, shutdown_handle, event, poll))]
    pub fn handle_event<T>(
        &mut self,
        waker: &Waker,
        shutdown_handle: &mut ShutdownHandle,
        event: &mio::event::Event,
        poll: &mut Poll,
    ) where
        T: std::fmt::Debug,
    {
        tracing::trace!(?event, "handle event");

        if event.token() == UDP_SOCKET_TOKEN {
            if event.is_writable() {
                self.handle_write();
            }

            if event.is_readable() {
                if let Some((buffer, addr)) = self.handle_read() {
                    let provider = Provider::new(
                        &mut self.buffer_pool,
                        &mut self.out_queue,
                        waker,
                        shutdown_handle,
                        &self.timer_sender,
                    );
                    self.dispatch.incoming(provider, buffer.as_ref(), addr);
                    self.buffer_pool.push(buffer);
                }
            }
        }

        self.update_interest(poll);
    }

    #[instrument(skip(self, poll))]
    fn update_interest(&mut self, poll: &mut Poll) {
        tracing::trace!("update interest");

        self.current_interest = if self.out_queue.is_empty() {
            Interest::READABLE
        } else {
            Interest::READABLE | Interest::WRITABLE
        };

        poll.registry()
            .reregister(&mut self.socket, UDP_SOCKET_TOKEN, self.current_interest)
            .unwrap();
    }
}
