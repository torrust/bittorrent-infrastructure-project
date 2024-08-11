use std::net::SocketAddr;

use tracing::instrument;
use umio::external::Sender;

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
    dispatcher: Sender<DispatchMessage>,
}

impl TrackerServer {
    /// Run a new `TrackerServer`.
    ///
    /// # Errors
    ///
    /// It would return an IO Error if unable to run the server.
    #[instrument(skip(), ret)]
    pub fn run<H>(bind: SocketAddr, handler: H) -> std::io::Result<TrackerServer>
    where
        H: ServerHandler + std::fmt::Debug + 'static,
    {
        tracing::info!("running server");

        let dispatcher = dispatcher::create_dispatcher(bind, handler)?;

        Ok(TrackerServer { dispatcher })
    }
}

impl Drop for TrackerServer {
    #[instrument(skip(self))]
    fn drop(&mut self) {
        tracing::debug!("server was dropped, sending shutdown notification...");

        self.dispatcher
            .send(DispatchMessage::Shutdown)
            .expect("bip_utracker: TrackerServer Failed To Send Shutdown Message");
    }
}
