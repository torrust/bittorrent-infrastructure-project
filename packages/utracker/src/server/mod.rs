use std::net::SocketAddr;
use std::sync::mpsc;

use tracing::{instrument, Level};
use umio::{MessageSender, ShutdownHandle};

use crate::server::dispatcher::DispatchMessage;
use crate::server::handler::ServerHandler;

mod dispatcher;
pub mod handler;

/// Tracker server that executes responses asynchronously.
///
/// Server will shutdown on drop.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct TrackerServer {
    dispatcher: MessageSender<DispatchMessage>,
    bound_socket: SocketAddr,
    shutdown_handle: ShutdownHandle,
}

impl TrackerServer {
    /// Run a new `TrackerServer`.
    ///
    /// # Errors
    ///
    /// It would return an IO Error if unable to run the server.
    #[instrument(skip(), ret(level = Level::TRACE))]
    pub fn run<H>(bind: SocketAddr, handler: H) -> std::io::Result<TrackerServer>
    where
        H: ServerHandler + std::fmt::Debug + 'static,
    {
        let (dispatcher, bound_socket, shutdown_handle) = dispatcher::create_dispatcher(bind, handler)?;

        tracing::info!(?bound_socket, "running server");

        Ok(TrackerServer {
            dispatcher,
            bound_socket,
            shutdown_handle,
        })
    }

    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.bound_socket
    }
}

impl Drop for TrackerServer {
    #[instrument(skip(self))]
    fn drop(&mut self) {
        tracing::info!("shutting down");
        let (shutdown_finished_sender, shutdown_finished_receiver) = mpsc::sync_channel(0);

        self.dispatcher
            .send(DispatchMessage::Shutdown(shutdown_finished_sender))
            .expect("bip_utracker: TrackerServer Failed To Send Shutdown Message");

        shutdown_finished_receiver.recv().unwrap().unwrap();
    }
}
