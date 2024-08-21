use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet, VecDeque};
use std::marker::PhantomData;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{mpsc, Arc, Condvar, Mutex, OnceLock, Weak};
use std::thread::JoinHandle;
use std::time::Instant;

use mio::net::UdpSocket;
use mio::{Events, Poll, Waker};
use tracing::{instrument, Level};

use crate::dispatcher::{DispatchHandler, Dispatcher};
use crate::provider::TimeoutAction;
use crate::WAKER_TOKEN;

const DEFAULT_BUFFER_SIZE: usize = 1500;
const DEFAULT_CHANNEL_CAPACITY: usize = 4096;
const DEFAULT_TIMER_CAPACITY: usize = 65536;

#[derive(Debug)]
pub struct MessageSender<T>
where
    T: std::fmt::Debug,
{
    sender: mpsc::Sender<T>,
    waker: Arc<Waker>,
}

impl<T> Clone for MessageSender<T>
where
    T: std::fmt::Debug,
{
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            waker: self.waker.clone(),
        }
    }
}

impl<T> MessageSender<T>
where
    T: std::fmt::Debug,
{
    #[instrument(skip(), ret(level = Level::TRACE))]
    fn new(sender: mpsc::Sender<T>, waker: Arc<Waker>) -> Self {
        Self { sender, waker }
    }

    #[instrument(skip(self))]
    pub fn send(&self, msg: T) -> Result<(), mpsc::SendError<T>> {
        tracing::trace!("sending message");

        let res = self.sender.send(msg);

        self.waker.wake().unwrap();

        res
    }
}

#[derive(Debug)]
struct Timeout<T> {
    when: Instant,
    token: Weak<T>,
}

impl<T> Ord for Timeout<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        other.when.cmp(&self.when) // from smallest to largest
    }
}

impl<T> PartialOrd for Timeout<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Eq for Timeout<T> {}

impl<T> PartialEq for Timeout<T> {
    fn eq(&self, other: &Self) -> bool {
        self.when == other.when
    }
}

#[derive(Debug)]
struct LoopWaker<T>
where
    T: std::fmt::Debug,
{
    waker: Arc<Waker>,
    active: HashSet<Arc<T>>,
    pending: Arc<(Mutex<BinaryHeap<Timeout<T>>>, Condvar)>,
    finished: Arc<Mutex<VecDeque<Arc<T>>>>,
    _handle: JoinHandle<()>,
}

impl<T> LoopWaker<T>
where
    T: std::hash::Hash + std::cmp::Eq + std::fmt::Debug + 'static,
    Weak<T>: Send,
    Arc<T>: Send,
{
    #[instrument(skip(waker, shutdown_handle), ret(level = Level::TRACE))]
    fn new(waker: Arc<Waker>, shutdown_handle: ShutdownHandle) -> Self {
        let pending: Arc<(Mutex<BinaryHeap<Timeout<T>>>, Condvar)> = Arc::default();
        let finished: Arc<Mutex<VecDeque<Arc<T>>>> = Arc::default();

        let handle = {
            let pending = pending.clone();
            let finished = finished.clone();
            let waker = waker.clone();

            std::thread::spawn(move || {
                let mut timeouts: BinaryHeap<Timeout<T>> = BinaryHeap::default();
                let mut elapsed = VecDeque::default();

                while !shutdown_handle.is_shutdown() {
                    {
                        let (lock, cvar) = &*pending;
                        let mut pending = lock.lock().unwrap();

                        while pending.is_empty() && timeouts.is_empty() {
                            pending = cvar.wait(pending).unwrap();

                            if shutdown_handle.is_shutdown() {
                                return;
                            }
                        }

                        timeouts.append(&mut pending);
                    }

                    while let Some(timeout) = timeouts.pop() {
                        let Some(token) = Weak::upgrade(&timeout.token) else {
                            continue;
                        };

                        match timeout.when.checked_duration_since(Instant::now()) {
                            Some(wait) => {
                                std::thread::sleep(wait);
                                elapsed.push_back(token);
                                break;
                            }
                            None => elapsed.push_back(token),
                        }
                    }

                    let mut finished = finished.lock().unwrap();
                    finished.append(&mut elapsed);
                    waker.wake().unwrap();
                }
            })
        };

        Self {
            waker,
            active: HashSet::default(),
            pending,
            finished,
            _handle: handle,
        }
    }

    #[instrument(skip(self))]
    fn next(&mut self) -> Option<T> {
        let token = self.finished.lock().unwrap().pop_front()?;

        let token = if self.remove(&token) { Arc::into_inner(token) } else { None };

        tracing::trace!(?token, "next timeout");

        token
    }

    #[instrument(skip(self))]
    fn remove(&mut self, token: &T) -> bool {
        let remove = self.active.remove(token);

        tracing::trace!(%remove, "removed timeout");

        remove
    }

    pub fn clear(&mut self) {
        self.active.clear();
        self.pending.0.lock().unwrap().clear();
    }

    #[instrument(skip(self))]
    fn push(&mut self, when: Instant, token: T) -> bool {
        let token = Arc::new(token);

        let timeout = Timeout {
            when,
            token: Arc::downgrade(&token),
        };

        let inserted = self.active.insert(token);

        if inserted {
            let (lock, cvar) = &*self.pending;

            lock.lock().unwrap().push(timeout);
            cvar.notify_one();
        };

        tracing::trace!(%inserted, "new timeout");

        inserted
    }
}

#[derive(Debug, Clone)]
pub struct ShutdownHandle {
    handle: Arc<OnceLock<()>>,
    waker: Arc<Waker>,
}

impl ShutdownHandle {
    #[instrument(skip(), ret(level = Level::TRACE))]
    fn new(waker: Arc<Waker>) -> Self {
        Self {
            handle: Arc::default(),
            waker,
        }
    }

    #[must_use]
    pub fn is_shutdown(&self) -> bool {
        self.handle.get().is_some()
    }

    #[instrument(skip(self))]
    pub fn shutdown(&mut self) {
        if self.handle.set(()).is_ok() {
            tracing::info!("shutdown called");
        } else {
            tracing::debug!("shutdown already called");
        };

        match self.waker.wake() {
            Ok(()) => tracing::trace!("waking... shutdown"),
            Err(e) => tracing::trace!("error waking... shutdown: {e}"),
        }
    }
}

impl Drop for ShutdownHandle {
    #[instrument(skip(self))]
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Debug)]
pub struct ELoopBuilder {
    channel_capacity: usize,
    timer_capacity: usize,
    buffer_size: usize,
    bind_address: SocketAddr,
}

impl ELoopBuilder {
    #[must_use]
    pub fn new() -> ELoopBuilder {
        Self::default()
    }

    #[must_use]
    pub fn channel_capacity(mut self, capacity: usize) -> ELoopBuilder {
        self.channel_capacity = capacity;
        self
    }

    #[must_use]
    pub fn timer_capacity(mut self, capacity: usize) -> ELoopBuilder {
        self.timer_capacity = capacity;
        self
    }

    #[must_use]
    pub fn bind_address(mut self, address: SocketAddr) -> ELoopBuilder {
        self.bind_address = address;
        self
    }

    #[must_use]
    pub fn buffer_length(mut self, length: usize) -> ELoopBuilder {
        self.buffer_size = length;
        self
    }

    /// Builds an `ELoop` instance with the specified configuration.
    ///
    /// # Errors
    ///
    /// This function will return an error if creating the `Poll` or `Waker` fails.
    pub fn build<D>(self) -> std::io::Result<(ELoop<D>, SocketAddr, ShutdownHandle)>
    where
        D: Dispatcher + std::fmt::Debug,
        <D as Dispatcher>::Message: std::fmt::Debug,
        <D as Dispatcher>::TimeoutToken: std::hash::Hash + std::cmp::Eq + std::fmt::Debug + 'static,
        Arc<<D as Dispatcher>::TimeoutToken>: Send,
        Weak<<D as Dispatcher>::TimeoutToken>: Send,
    {
        ELoop::from_builder(&self)
    }
}

impl Default for ELoopBuilder {
    fn default() -> Self {
        let default_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0));

        ELoopBuilder {
            channel_capacity: DEFAULT_CHANNEL_CAPACITY,
            timer_capacity: DEFAULT_TIMER_CAPACITY,
            buffer_size: DEFAULT_BUFFER_SIZE,
            bind_address: default_addr,
        }
    }
}

#[derive(Debug)]
pub struct ELoop<D>
where
    D: Dispatcher,
    <D as Dispatcher>::Message: std::fmt::Debug,
{
    buffer_size: usize,
    socket: Option<UdpSocket>,
    poll: Poll,
    events: Events,
    loop_waker: LoopWaker<D::TimeoutToken>,
    shutdown_handle: ShutdownHandle,
    message_sender: MessageSender<D::Message>,
    message_receiver: mpsc::Receiver<D::Message>,
    timeout_sender: mpsc::Sender<TimeoutAction<D::TimeoutToken>>,
    timeout_receiver: mpsc::Receiver<TimeoutAction<D::TimeoutToken>>,
    _marker: PhantomData<D>,
}

impl<D> ELoop<D>
where
    D: Dispatcher + std::fmt::Debug,
    <D as Dispatcher>::Message: std::fmt::Debug,
    <D as Dispatcher>::TimeoutToken: std::hash::Hash + std::cmp::Eq + std::fmt::Debug + 'static,
    Arc<<D as Dispatcher>::TimeoutToken>: Send,
    Weak<<D as Dispatcher>::TimeoutToken>: Send,
{
    #[instrument(skip(), err, ret(level = Level::TRACE))]
    fn from_builder(builder: &ELoopBuilder) -> std::io::Result<(ELoop<D>, SocketAddr, ShutdownHandle)> {
        let poll = Poll::new()?;
        let events = Events::with_capacity(builder.channel_capacity);

        let (message_sender, message_receiver) = mpsc::channel();
        let (timeout_sender, timeout_receiver) = mpsc::channel();

        let socket = UdpSocket::bind(builder.bind_address)?;

        let bound_socket = socket.local_addr()?;

        let waker = Arc::new(Waker::new(poll.registry(), WAKER_TOKEN)?);

        let shutdown_handle = ShutdownHandle::new(waker.clone());

        let loop_waker = LoopWaker::new(waker.clone(), shutdown_handle.clone());
        let message_sender = MessageSender::new(message_sender, waker);

        Ok((
            ELoop {
                buffer_size: builder.buffer_size,
                socket: Some(socket),
                poll,
                events,
                loop_waker,
                shutdown_handle: shutdown_handle.clone(),
                message_sender,
                message_receiver,
                timeout_sender,
                timeout_receiver,
                _marker: PhantomData,
            },
            bound_socket,
            shutdown_handle,
        ))
    }

    #[must_use]
    pub fn waker(&self) -> &Waker {
        &self.loop_waker.waker
    }

    /// Creates a channel for sending messages to the event loop.
    #[must_use]
    #[instrument(skip(self))]
    pub fn channel(&self) -> MessageSender<<D as Dispatcher>::Message> {
        self.message_sender.clone()
    }

    /// Runs the event loop with the provided dispatcher.
    ///
    /// # Errors
    ///
    /// This function will return an error if binding the UDP socket or polling events fails.
    #[instrument(skip(self, dispatcher))]
    pub fn run(&mut self, dispatcher: D, started_eloop_sender: mpsc::SyncSender<std::io::Result<()>>) -> std::io::Result<()>
    where
        D: std::fmt::Debug,
        <D as Dispatcher>::Message: std::fmt::Debug,
        <D as Dispatcher>::TimeoutToken: std::hash::Hash + std::cmp::Eq + std::fmt::Debug + 'static,
    {
        let mut dispatch_handler = DispatchHandler::new(
            self.socket.take().unwrap(),
            self.buffer_size,
            dispatcher,
            &mut self.poll,
            self.timeout_sender.clone(),
        );

        let () = started_eloop_sender
            .send(Ok(()))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::BrokenPipe, e))?;

        loop {
            if self.shutdown_handle.is_shutdown() {
                self.loop_waker.clear();
                tracing::debug!("shutting down...");
                break;
            }

            // Handle timeouts
            while let Some(token) = self.loop_waker.next() {
                dispatch_handler.handle_timeout(&self.loop_waker.waker, &mut self.shutdown_handle, token);
            }

            // Handle events
            for event in &self.events {
                dispatch_handler.handle_event::<D>(&self.loop_waker.waker, &mut self.shutdown_handle, event, &mut self.poll);
            }

            // Handle messages
            while let Ok(message) = self.message_receiver.try_recv() {
                dispatch_handler.handle_message(&self.loop_waker.waker, &mut self.shutdown_handle, message);
            }

            // Add timeouts
            while let Ok(action) = self.timeout_receiver.try_recv() {
                match action {
                    TimeoutAction::Add { token, when } => {
                        tracing::trace!(?token, ?when, "set timeout");
                        self.loop_waker.push(when, token);
                    }
                    TimeoutAction::Remove { token } => {
                        tracing::trace!(?token, "clear timeout");
                        self.loop_waker.remove(&token);
                    }
                }
            }

            self.poll.poll(&mut self.events, None)?;
        }

        Ok(())
    }
}
