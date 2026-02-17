use std::net::SocketAddr;
use std::sync::mpsc;

use nom::IResult;
use tokio::net::UdpSocket;
use tokio::runtime::Builder;
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::instrument;

use crate::announce::AnnounceRequest;
use crate::error::ErrorResponse;
use crate::request::{self, RequestType, TrackerRequest};
use crate::response::{ResponseType, TrackerResponse};
use crate::runtime::{channel, MessageSender, ShutdownHandle};
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
    H: ServerHandler + std::fmt::Debug + Send + 'static,
{
    tracing::trace!("create dispatcher");

    // Bind synchronously so we can return the bound address immediately.
    let std_socket = std::net::UdpSocket::bind(bind)?;
    std_socket.set_nonblocking(true)?;
    let local_addr = std_socket.local_addr()?;

    let (tx, rx) = channel();

    let handle = std::thread::spawn(move || {
        let rt = Builder::new_current_thread().enable_all().build().expect("tokio runtime");
        rt.block_on(run_server(std_socket, handler, rx));
    });

    Ok((tx, local_addr, handle))
}

#[instrument(skip(socket, handler, rx))]
async fn run_server<H>(socket: std::net::UdpSocket, mut handler: H, mut rx: UnboundedReceiver<DispatchMessage>)
where
    H: ServerHandler + std::fmt::Debug + Send + 'static,
{
    let socket = UdpSocket::from_std(socket).expect("convert socket");
    let mut buf = vec![0u8; EXPECTED_PACKET_LENGTH];

    loop {
        tokio::select! {
            res = socket.recv_from(&mut buf) => {
                match res {
                    Ok((size, addr)) => {
                        match TrackerRequest::from_bytes(&buf[..size]) {
                            IResult::Ok((_, request)) => {
                                process_request(&socket, &mut handler, &request, addr).await;
                            }
                            Err(e) => {
                                tracing::error!(%e, "failed to parse incoming request");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(%e, "error reading from socket");
                    }
                }
            }
            Some(message) = rx.recv() => {
                match message {
                    DispatchMessage::Shutdown(shutdown_finished_sender) => {
                        tracing::debug!("received a shutdown notification");
                        drop(shutdown_finished_sender.send(Ok(())));
                        break;
                    }
                }
            }
            else => break,
        }
    }
}

#[instrument(skip(socket, handler, request))]
async fn process_request<H>(socket: &UdpSocket, handler: &mut H, request: &TrackerRequest<'_>, addr: SocketAddr)
where
    H: ServerHandler + std::fmt::Debug + Send + 'static,
{
    tracing::trace!("process request");

    let conn_id = request.connection_id();
    let trans_id = request.transaction_id();

    let response_type = match request.request_type() {
        &RequestType::Connect => {
            if conn_id == request::CONNECT_ID_PROTOCOL_ID {
                forward_connect(handler, addr, trans_id)
            } else {
                tracing::warn!(
                    "request was not `CONNECT_ID_PROTOCOL_ID`, i.e. {}, but {conn_id}.",
                    request::CONNECT_ID_PROTOCOL_ID
                );
                return;
            }
        }
        RequestType::Announce(req) => forward_announce(handler, trans_id, conn_id, req, addr),
        RequestType::Scrape(req) => forward_scrape(handler, trans_id, conn_id, req, addr),
    };

    if let Some(resp) = response_type {
        write_response(socket, &resp, addr).await;
    }
}

#[instrument(skip(handler))]
fn forward_connect<H>(handler: &mut H, addr: SocketAddr, trans_id: u32) -> Option<TrackerResponse<'static>>
where
    H: ServerHandler + std::fmt::Debug + Send + 'static,
{
    let Some(attempt) = handler.connect(addr) else {
        tracing::warn!("connect attempt canceled");
        return None;
    };

    let response_type = match attempt {
        Ok(conn_id) => ResponseType::Connect(conn_id),
        Err(err_msg) => ResponseType::Error(ErrorResponse::new(err_msg).to_owned()),
    };

    let response = TrackerResponse::new(trans_id, response_type);
    tracing::trace!(?response, "forward connect");
    Some(response)
}

#[instrument(skip(handler, request))]
fn forward_announce<H>(
    handler: &mut H,
    trans_id: u32,
    conn_id: u64,
    request: &AnnounceRequest<'_>,
    addr: SocketAddr,
) -> Option<TrackerResponse<'static>>
where
    H: ServerHandler + std::fmt::Debug + Send + 'static,
{
    let Some(attempt) = handler.announce(addr, conn_id, request) else {
        tracing::warn!("announce attempt canceled");
        return None;
    };

    let response_type = match attempt {
        Ok(response) => ResponseType::Announce(response.to_owned()),
        Err(err_msg) => ResponseType::Error(ErrorResponse::new(err_msg).to_owned()),
    };
    let response = TrackerResponse::new(trans_id, response_type);
    tracing::trace!(?response, "forward announce");
    Some(response)
}

#[instrument(skip(handler, request))]
fn forward_scrape<H>(
    handler: &mut H,
    trans_id: u32,
    conn_id: u64,
    request: &ScrapeRequest<'_>,
    addr: SocketAddr,
) -> Option<TrackerResponse<'static>>
where
    H: ServerHandler + std::fmt::Debug + Send + 'static,
{
    tracing::debug!("forward scrape");

    let Some(attempt) = handler.scrape(addr, conn_id, request) else {
        tracing::warn!("connect scrape canceled");
        return None;
    };

    let response_type = match attempt {
        Ok(response) => ResponseType::Scrape(response.to_owned()),
        Err(err_msg) => ResponseType::Error(ErrorResponse::new(err_msg).to_owned()),
    };

    let response = TrackerResponse::new(trans_id, response_type);
    Some(response)
}

/// Write the given tracker response to the socket.
#[instrument(skip(socket, response))]
async fn write_response(socket: &UdpSocket, response: &TrackerResponse<'_>, addr: SocketAddr) {
    tracing::debug!("write response");
    let mut buf = Vec::with_capacity(EXPECTED_PACKET_LENGTH);
    match response.write_bytes(&mut buf) {
        Ok(()) => {
            if let Err(e) = socket.send_to(&buf, addr).await {
                tracing::error!(%e, "error writing response to socket");
            }
        }
        Err(e) => tracing::error!(%e, "error serializing response"),
    }
}
