use std::net::SocketAddr;
use std::sync::mpsc;

use nom::IResult;
use tracing::{instrument, Level};
use umio::{Dispatcher, ELoopBuilder, MessageSender, Provider, ShutdownHandle};

use crate::announce::AnnounceRequest;
use crate::error::ErrorResponse;
use crate::request::{self, RequestType, TrackerRequest};
use crate::response::{ResponseType, TrackerResponse};
use crate::scrape::ScrapeRequest;
use crate::server::handler::ServerHandler;

const EXPECTED_PACKET_LENGTH: usize = 1500;

/// Internal dispatch message for servers.
#[derive(Debug)]
pub enum DispatchMessage {
    Shutdown(mpsc::SyncSender<std::io::Result<()>>),
}

/// Create a new background dispatcher to service requests.
#[allow(clippy::module_name_repetitions)]
#[instrument(skip())]
pub fn create_dispatcher<H>(
    bind: SocketAddr,
    handler: H,
) -> std::io::Result<(MessageSender<DispatchMessage>, SocketAddr, ShutdownHandle)>
where
    H: ServerHandler + std::fmt::Debug + 'static,
{
    tracing::trace!("create dispatcher");

    let builder = ELoopBuilder::new()
        .channel_capacity(1)
        .timer_capacity(0)
        .bind_address(bind)
        .buffer_length(EXPECTED_PACKET_LENGTH);

    let (mut eloop, socket, shutdown) = builder.build()?;
    let channel = eloop.channel();

    let dispatcher = ServerDispatcher::new(handler);

    let handle = {
        let (started_eloop_sender, started_eloop_receiver) = mpsc::sync_channel(0);

        let handle = std::thread::spawn(move || {
            eloop.run(dispatcher, started_eloop_sender).unwrap();
        });

        let () = started_eloop_receiver
            .recv()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::BrokenPipe, e))??;

        handle
    };

    Ok((channel, socket, shutdown))
}

// ----------------------------------------------------------------------------//

/// Dispatcher that executes requests asynchronously.
#[derive(Debug)]
struct ServerDispatcher<H>
where
    H: ServerHandler + std::fmt::Debug,
{
    handler: H,
}

impl<H> ServerDispatcher<H>
where
    H: ServerHandler + std::fmt::Debug,
{
    /// Create a new `ServerDispatcher`.
    #[instrument(skip(), ret(level = Level::TRACE))]
    fn new(handler: H) -> ServerDispatcher<H> {
        ServerDispatcher { handler }
    }

    /// Forward the request on to the appropriate handler method.
    #[instrument(skip(self, provider))]
    fn process_request(
        &mut self,
        provider: &mut Provider<'_, ServerDispatcher<H>>,
        request: &TrackerRequest<'_>,
        addr: SocketAddr,
    ) {
        tracing::trace!("process request");

        let conn_id = request.connection_id();
        let trans_id = request.transaction_id();

        match request.request_type() {
            &RequestType::Connect => {
                if conn_id == request::CONNECT_ID_PROTOCOL_ID {
                    self.forward_connect(provider, trans_id, addr);
                } else {
                    tracing::warn!(
                        "request was not `CONNECT_ID_PROTOCOL_ID`, i.e. {}, but {conn_id}.",
                        request::CONNECT_ID_PROTOCOL_ID
                    );
                }
            }
            RequestType::Announce(req) => {
                self.forward_announce(provider, trans_id, conn_id, req, addr);
            }
            RequestType::Scrape(req) => {
                self.forward_scrape(provider, trans_id, conn_id, req, addr);
            }
        };
    }

    /// Forward a connect request on to the appropriate handler method.
    #[instrument(skip(self, provider))]
    fn forward_connect(&mut self, provider: &mut Provider<'_, ServerDispatcher<H>>, trans_id: u32, addr: SocketAddr) {
        let Some(attempt) = self.handler.connect(addr) else {
            tracing::warn!("connect attempt canceled");

            return;
        };

        let response_type = match attempt {
            Ok(conn_id) => ResponseType::Connect(conn_id),
            Err(err_msg) => ResponseType::Error(ErrorResponse::new(err_msg)),
        };

        let response = TrackerResponse::new(trans_id, response_type);

        tracing::trace!(?response, "forward connect");

        write_response(provider, &response, addr);
    }

    /// Forward an announce request on to the appropriate handler method.
    #[instrument(skip(self, provider))]
    fn forward_announce(
        &mut self,
        provider: &mut Provider<'_, ServerDispatcher<H>>,
        trans_id: u32,
        conn_id: u64,
        request: &AnnounceRequest<'_>,
        addr: SocketAddr,
    ) {
        let Some(attempt) = self.handler.announce(addr, conn_id, request) else {
            tracing::warn!("announce attempt canceled");

            return;
        };

        let response_type = match attempt {
            Ok(response) => ResponseType::Announce(response),
            Err(err_msg) => ResponseType::Error(ErrorResponse::new(err_msg)),
        };
        let response = TrackerResponse::new(trans_id, response_type);

        tracing::trace!(?response, "forward announce");

        write_response(provider, &response, addr);
    }

    /// Forward a scrape request on to the appropriate handler method.
    #[instrument(skip(self, provider))]
    fn forward_scrape(
        &mut self,
        provider: &mut Provider<'_, ServerDispatcher<H>>,
        trans_id: u32,
        conn_id: u64,
        request: &ScrapeRequest<'_>,
        addr: SocketAddr,
    ) {
        tracing::debug!("forward scrape");

        let Some(attempt) = self.handler.scrape(addr, conn_id, request) else {
            tracing::warn!("connect scrape canceled");

            return;
        };

        let response_type = match attempt {
            Ok(response) => ResponseType::Scrape(response),
            Err(err_msg) => ResponseType::Error(ErrorResponse::new(err_msg)),
        };

        let response = TrackerResponse::new(trans_id, response_type);

        write_response(provider, &response, addr);
    }
}

/// Write the given tracker response through to the given provider.
#[instrument(skip(provider))]
fn write_response<H>(provider: &mut Provider<'_, ServerDispatcher<H>>, response: &TrackerResponse<'_>, addr: SocketAddr)
where
    H: ServerHandler + std::fmt::Debug,
{
    tracing::debug!("write response");

    provider.set_dest(addr);

    match response.write_bytes(provider) {
        Ok(()) => (),
        Err(e) => {
            tracing::error!(%e, "error writing response to cursor");
        }
    }
}

impl<H> Dispatcher for ServerDispatcher<H>
where
    H: ServerHandler + std::fmt::Debug,
{
    type TimeoutToken = ();
    type Message = DispatchMessage;

    #[instrument(skip(self, provider))]
    fn incoming(&mut self, mut provider: Provider<'_, Self>, message: &[u8], addr: SocketAddr) {
        let () = match TrackerRequest::from_bytes(message) {
            IResult::Ok((_, request)) => {
                tracing::debug!("received an incoming request: {request:?}");

                self.process_request(&mut provider, &request, addr);
            }
            Err(e) => {
                tracing::error!(%e, "received an incoming error message");
            }
        };
    }

    #[instrument(skip(self, provider))]
    fn notify(&mut self, mut provider: Provider<'_, Self>, message: DispatchMessage) {
        let () = match message {
            DispatchMessage::Shutdown(shutdown_finished_sender) => {
                tracing::debug!("received a shutdown notification");

                provider.shutdown();

                let () = shutdown_finished_sender.send(Ok(())).unwrap();
            }
        };
    }

    #[instrument(skip(self))]
    fn timeout(&mut self, _: Provider<'_, Self>, (): ()) {
        tracing::error!("timeout not yet supported!");
        unimplemented!();
    }
}
