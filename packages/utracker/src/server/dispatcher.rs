use std::net::SocketAddr;

use nom::IResult;
use tracing::instrument;
use umio::external::Sender;
use umio::{Dispatcher, ELoopBuilder, Provider};

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
    Shutdown,
}

/// Create a new background dispatcher to service requests.
#[allow(clippy::module_name_repetitions)]
#[instrument(skip())]
pub fn create_dispatcher<H>(bind: SocketAddr, handler: H) -> std::io::Result<Sender<DispatchMessage>>
where
    H: ServerHandler + std::fmt::Debug + 'static,
{
    tracing::debug!("create dispatcher");

    let builder = ELoopBuilder::new()
        .channel_capacity(1)
        .timer_capacity(0)
        .bind_address(bind)
        .buffer_length(EXPECTED_PACKET_LENGTH);

    let mut eloop = builder.build()?;
    let channel = eloop.channel();

    let dispatch = ServerDispatcher::new(handler);

    std::thread::spawn(move || {
        eloop.run(dispatch).expect("bip_utracker: ELoop Shutdown Unexpectedly...");
    });

    Ok(channel)
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
    #[instrument(skip(), ret)]
    fn new(handler: H) -> ServerDispatcher<H> {
        tracing::debug!("new");

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
        tracing::debug!("process request");

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
        tracing::debug!("forward connect");

        let Some(attempt) = self.handler.connect(addr) else {
            tracing::warn!("connect attempt canceled");

            return;
        };

        let response_type = match attempt {
            Ok(conn_id) => ResponseType::Connect(conn_id),
            Err(err_msg) => ResponseType::Error(ErrorResponse::new(err_msg)),
        };

        let response = TrackerResponse::new(trans_id, response_type);

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
        tracing::debug!("forward announce");

        let Some(attempt) = self.handler.announce(addr, conn_id, request) else {
            tracing::warn!("announce attempt canceled");

            return;
        };

        let response_type = match attempt {
            Ok(response) => ResponseType::Announce(response),
            Err(err_msg) => ResponseType::Error(ErrorResponse::new(err_msg)),
        };
        let response = TrackerResponse::new(trans_id, response_type);

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

    provider.outgoing(|buffer| {
        let mut cursor = std::io::Cursor::new(buffer);

        match response.write_bytes(&mut cursor) {
            Ok(()) => Some((cursor.position().try_into().unwrap(), addr)),
            Err(e) => {
                tracing::error!("error writing response to cursor: {e}");
                None
            }
        }
    });
}

impl<H> Dispatcher for ServerDispatcher<H>
where
    H: ServerHandler + std::fmt::Debug,
{
    type Timeout = ();
    type Message = DispatchMessage;

    #[instrument(skip(self, provider))]
    fn incoming(&mut self, mut provider: Provider<'_, Self>, message: &[u8], addr: SocketAddr) {
        let () = match TrackerRequest::from_bytes(message) {
            IResult::Ok((_, request)) => {
                tracing::debug!("received an incoming request: {request:?}");

                self.process_request(&mut provider, &request, addr);
            }
            Err(e) => {
                tracing::error!("received an incoming error message: {e}");
            }
        };
    }

    #[instrument(skip(self, provider))]
    fn notify(&mut self, mut provider: Provider<'_, Self>, message: DispatchMessage) {
        let () = match message {
            DispatchMessage::Shutdown => {
                tracing::debug!("received a shutdown notification");

                provider.shutdown();
            }
        };
    }

    #[instrument(skip(self))]
    fn timeout(&mut self, _: Provider<'_, Self>, (): ()) {
        tracing::error!("timeout not yet supported!");
        unimplemented!();
    }
}
