use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use std::sync::mpsc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use futures::executor::block_on;
use futures::future::{BoxFuture, Either};
use futures::sink::Sink;
use futures::{FutureExt, SinkExt};
use handshake::{DiscoveryInfo, InitiateMessage, Protocol};
use nom::IResult;
use tracing::{instrument, Level};
use umio::{Dispatcher, ELoopBuilder, MessageSender, Provider, ShutdownHandle};
use util::bt::PeerId;

use super::HandshakerMessage;
use crate::announce::{AnnounceRequest, DesiredPeers, SourceIP};
use crate::client::error::{ClientError, ClientResult};
use crate::client::{ClientMetadata, ClientRequest, ClientResponse, ClientToken, RequestLimiter};
use crate::option::AnnounceOptions;
use crate::request::{self, RequestType, TrackerRequest};
use crate::response::{ResponseType, TrackerResponse};
use crate::scrape::ScrapeRequest;

const EXPECTED_PACKET_LENGTH: usize = 1500;

const CONNECTION_ID_VALID_DURATION_MILLIS: i64 = 60000;
const MAXIMUM_REQUEST_RETRANSMIT_ATTEMPTS: u64 = 8;

/// Internal dispatch timeout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum DispatchTimeout {
    Connect(ClientToken),
    CleanUp,
}

impl Default for DispatchTimeout {
    fn default() -> Self {
        Self::CleanUp
    }
}

#[derive(Default, Clone, Copy, Debug)]
struct TimeoutToken {
    id: TimeoutId,
    dispatch: DispatchTimeout,
}

impl TimeoutToken {
    fn new(dispatch: DispatchTimeout) -> (Self, TimeoutId) {
        let id = TimeoutId::default();
        (Self { id, dispatch }, id)
    }

    fn cleanup(id: TimeoutId) -> Self {
        Self {
            id,
            dispatch: DispatchTimeout::CleanUp,
        }
    }
}

impl Ord for TimeoutToken {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

impl PartialOrd for TimeoutToken {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for TimeoutToken {}

impl PartialEq for TimeoutToken {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl std::hash::Hash for TimeoutToken {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// Internal dispatch message for clients.
#[derive(Debug)]
pub enum DispatchMessage {
    Request(SocketAddr, ClientToken, ClientRequest),
    StartTimer,
    Shutdown(mpsc::SyncSender<std::io::Result<()>>),
}

/// Create a new background dispatcher to execute request and send responses back.
///
/// Assumes `msg_capacity` is less than `usize::max_value`().
#[allow(clippy::module_name_repetitions)]
#[instrument(skip())]
pub fn create_dispatcher<H>(
    bind: SocketAddr,
    handshaker: H,
    msg_capacity: usize,
    limiter: RequestLimiter,
) -> std::io::Result<(MessageSender<DispatchMessage>, SocketAddr, ShutdownHandle)>
where
    H: Sink<std::io::Result<HandshakerMessage>> + std::fmt::Debug + DiscoveryInfo + Send + Unpin + 'static,
    H::Error: std::fmt::Display,
{
    tracing::debug!("creating dispatcher");

    // Timer capacity is plus one for the cache cleanup timer
    let builder = ELoopBuilder::new()
        .channel_capacity(msg_capacity)
        .timer_capacity(msg_capacity + 1)
        .bind_address(bind)
        .buffer_length(EXPECTED_PACKET_LENGTH);

    let (mut eloop, socket, shutdown) = builder.build()?;
    let channel = eloop.channel();

    let dispatcher = ClientDispatcher::new(handshaker, bind, limiter);

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

    channel
        .send(DispatchMessage::StartTimer)
        .expect("bip_utracker: ELoop Failed To Start Connect ID Timer...");

    Ok((channel, socket, shutdown))
}

// ----------------------------------------------------------------------------//

/// Dispatcher that executes requests asynchronously.
#[derive(Debug)]
struct ClientDispatcher<H> {
    handshaker: H,
    pid: PeerId,
    port: u16,
    bound_addr: SocketAddr,
    active_requests: HashMap<ClientToken, ConnectTimer>,
    id_cache: ConnectIdCache,
    limiter: RequestLimiter,
}

impl<H> ClientDispatcher<H>
where
    H: Sink<std::io::Result<HandshakerMessage>> + std::fmt::Debug + DiscoveryInfo + Send + Unpin + 'static,
    H::Error: std::fmt::Display,
{
    /// Create a new `ClientDispatcher`.
    #[instrument(skip(), ret(level = Level::TRACE))]
    pub fn new(handshaker: H, bind: SocketAddr, limiter: RequestLimiter) -> ClientDispatcher<H> {
        tracing::debug!("new client dispatcher");

        let peer_id = handshaker.peer_id();
        let port = handshaker.port();

        ClientDispatcher {
            handshaker,
            pid: peer_id,
            port,
            bound_addr: bind,
            active_requests: HashMap::new(),
            id_cache: ConnectIdCache::new(),
            limiter,
        }
    }

    /// Shutdown the current dispatcher, notifying all pending requests.
    #[instrument(skip(self, provider), fields(unfinished_requests= %self.active_requests.len()))]
    pub fn shutdown(&mut self, provider: &mut Provider<'_, ClientDispatcher<H>>) {
        tracing::debug!("shuting down...");

        let mut active_requests = std::mem::take(&mut self.active_requests);
        let mut unfinished_requests = active_requests.drain();

        // Notify all active requests with the appropriate error
        for (client_token, connect_timer) in unfinished_requests.by_ref() {
            tracing::trace!(?client_token, ?connect_timer, "removing...");

            if let Some(id) = connect_timer.timeout_id() {
                provider.remove_timeout(TimeoutToken::cleanup(id)).unwrap();
            }

            self.notify_client(client_token, Err(ClientError::ClientShutdown));
        }
        provider.shutdown();
    }

    /// Finish a request by sending the result back to the client.
    #[instrument(skip(self))]
    pub fn notify_client(&mut self, token: ClientToken, result: ClientResult<ClientResponse>) {
        tracing::trace!("notifying clients");

        match block_on(self.handshaker.send(Ok(ClientMetadata::new(token, result).into()))) {
            Ok(()) => tracing::debug!("client metadata sent"),
            Err(e) => tracing::error!("sending client metadata failed with error: {e}"),
        }

        self.limiter.acknowledge();
    }

    /// Process a request to be sent to the given address and associated with the given token.
    #[instrument(skip(self, provider, addr, token, request))]
    pub fn send_request(
        &mut self,
        provider: &mut Provider<'_, ClientDispatcher<H>>,
        addr: SocketAddr,
        token: ClientToken,
        request: ClientRequest,
    ) {
        tracing::debug!(?addr, ?token, ?request, "sending request");

        let bound_addr = self.bound_addr;

        // Check for IP version mismatch between source addr and dest addr
        match (bound_addr, addr) {
            (SocketAddr::V4(_), SocketAddr::V6(_)) | (SocketAddr::V6(_), SocketAddr::V4(_)) => {
                tracing::error!(%bound_addr, %addr, "ip version mismatch between bound address and address");

                self.notify_client(token, Err(ClientError::IPVersionMismatch));

                return;
            }
            _ => (),
        };
        self.active_requests.insert(token, ConnectTimer::new(addr, request));

        self.process_request(provider, token, false);
    }

    /// Process a response received from some tracker and match it up against our sent requests.
    #[instrument(skip(self, provider, response, addr))]
    pub fn recv_response(
        &mut self,
        provider: &mut Provider<'_, ClientDispatcher<H>>,
        response: &TrackerResponse<'_>,
        addr: SocketAddr,
    ) {
        tracing::debug!(?response, ?addr, "receiving response");

        let token = ClientToken(response.transaction_id());

        let conn_timer = if let Some(conn_timer) = self.active_requests.remove(&token) {
            if conn_timer.message_params().0 == addr {
                conn_timer
            } else {
                tracing::error!(?conn_timer, %addr, "different message prams");

                return;
            }
        } else {
            tracing::error!(?token, "token not in active requests");

            return;
        };

        if let Some(clear_timeout_token) = conn_timer.timeout_id().map(TimeoutToken::cleanup) {
            provider
                .remove_timeout(clear_timeout_token)
                .expect("bip_utracker: Failed To Clear Request Timeout");
        };

        // Check if the response requires us to update the connection timer
        if let &ResponseType::Connect(id) = response.response_type() {
            self.id_cache.put(addr, id);

            self.active_requests.insert(token, conn_timer);
            self.process_request(provider, token, false);
        } else {
            // Match the request type against the response type and update our client
            match (conn_timer.message_params().1, response.response_type()) {
                (&ClientRequest::Announce(hash, _), ResponseType::Announce(res)) => {
                    // Forward contact information on to the handshaker
                    for addr in res.peers().iter() {
                        tracing::debug!("sending will block if unable to send!");
                        match block_on(
                            self.handshaker
                                .send(Ok(InitiateMessage::new(Protocol::BitTorrent, hash, addr).into())),
                        ) {
                            Ok(()) => tracing::debug!("handshake for: {addr} initiated"),
                            Err(e) => tracing::warn!("handshake for: {addr} failed with: {e}"),
                        }
                    }

                    self.notify_client(token, Ok(ClientResponse::Announce(res.to_owned())));
                }
                (&ClientRequest::Scrape(..), ResponseType::Scrape(res)) => {
                    self.notify_client(token, Ok(ClientResponse::Scrape(res.to_owned())));
                }
                (_, ResponseType::Error(res)) => {
                    self.notify_client(token, Err(ClientError::ServerMessage(res.to_owned())));
                }
                _ => {
                    self.notify_client(token, Err(ClientError::ServerError));
                }
            }
        }
    }

    /// Process an existing request, either re requesting a connection id or sending the actual request again.
    ///
    /// If this call is the result of a timeout, that will decide whether to cancel the request or not.
    #[instrument(skip(self, provider, token, timed_out))]
    fn process_request(&mut self, provider: &mut Provider<'_, ClientDispatcher<H>>, token: ClientToken, timed_out: bool) {
        tracing::debug!(?token, ?timed_out, "processing request");

        let Some(mut conn_timer) = self.active_requests.remove(&token) else {
            tracing::error!(?token, "token not in active requests");

            return;
        };

        // Resolve the duration of the current timeout to use
        let Some(next_timeout) = conn_timer.current_timeout(timed_out) else {
            let err = ClientError::MaxTimeout;

            tracing::error!("error reached timeout: {err}");

            self.notify_client(token, Err(err));

            return;
        };

        let addr = conn_timer.message_params().0;
        let opt_conn_id = self.id_cache.get(conn_timer.message_params().0);

        // Resolve the type of request we need to make
        let (conn_id, request_type) = match (opt_conn_id, conn_timer.message_params().1) {
            (Some(id), &ClientRequest::Announce(hash, state)) => {
                let source_ip = match addr {
                    SocketAddr::V4(_) => SourceIP::ImpliedV4,
                    SocketAddr::V6(_) => SourceIP::ImpliedV6,
                };
                let key = rand::random::<u32>();

                (
                    id,
                    RequestType::Announce(AnnounceRequest::new(
                        hash,
                        self.pid,
                        state,
                        source_ip,
                        key,
                        DesiredPeers::Default,
                        self.port,
                        AnnounceOptions::new(),
                    )),
                )
            }
            (Some(id), &ClientRequest::Scrape(hash)) => {
                let mut scrape_request = ScrapeRequest::new();
                scrape_request.insert(hash);

                (id, RequestType::Scrape(scrape_request))
            }
            (None, _) => (request::CONNECT_ID_PROTOCOL_ID, RequestType::Connect),
        };
        let tracker_request = TrackerRequest::new(conn_id, token.0, request_type);

        // Try to write the request out to the server
        let mut write_success = false;
        provider.set_dest(addr);

        {
            match tracker_request.write_bytes(provider) {
                Ok(()) => {
                    write_success = true;
                }
                Err(e) => {
                    tracing::error!(?e, "failed to write out the tracker request with error");
                }
            };
        }

        let next_timeout_at = Instant::now().checked_add(Duration::from_millis(next_timeout)).unwrap();

        let (timeout_token, timeout_id) = TimeoutToken::new(DispatchTimeout::Connect(token));

        let () = provider
            .set_timeout(timeout_token, next_timeout_at)
            .expect("bip_utracker: Failed To Set Timeout For Request");

        // If message was not sent (too long to fit) then end the request
        if write_success {
            conn_timer.set_timeout_id(timeout_id);

            self.active_requests.insert(token, conn_timer);
        } else {
            let e = ClientError::MaxLength;
            tracing::warn!(?e, "notifying client with error");

            self.notify_client(token, Err(e));
        }
    }
}

impl<H> Dispatcher for ClientDispatcher<H>
where
    H: Sink<std::io::Result<HandshakerMessage>> + std::fmt::Debug + DiscoveryInfo + Send + Unpin + 'static,
    H::Error: std::fmt::Display,
{
    type TimeoutToken = TimeoutToken;
    type Message = DispatchMessage;

    #[instrument(skip(self, provider, message, addr))]
    fn incoming(&mut self, mut provider: Provider<'_, Self>, message: &[u8], addr: SocketAddr) {
        tracing::debug!(?message, %addr, "received incoming");

        let () = match TrackerResponse::from_bytes(message) {
            IResult::Ok((_, response)) => {
                tracing::trace!(?response, %addr, "received an incoming response");

                self.recv_response(&mut provider, &response, addr);
            }
            Err(e) => {
                tracing::error!(%e, "received an incoming error message");
            }
        };
    }

    #[instrument(skip(self, provider, message))]
    fn notify(&mut self, mut provider: Provider<'_, Self>, message: DispatchMessage) {
        tracing::debug!(?message, "received notify");

        match message {
            DispatchMessage::Request(addr, token, req_type) => {
                self.send_request(&mut provider, addr, token, req_type);
            }
            DispatchMessage::StartTimer => self.timeout(provider, TimeoutToken::default()),
            DispatchMessage::Shutdown(shutdown_finished_sender) => {
                self.shutdown(&mut provider);

                let () = shutdown_finished_sender.send(Ok(())).unwrap();
            }
        }
    }

    #[instrument(skip(self, provider, timeout))]
    fn timeout(&mut self, mut provider: Provider<'_, Self>, timeout: TimeoutToken) {
        tracing::debug!(?timeout, "received timeout");

        match timeout.dispatch {
            DispatchTimeout::Connect(token) => self.process_request(&mut provider, token, true),
            DispatchTimeout::CleanUp => {
                self.id_cache.clean_expired();

                let next_timeout_at = Instant::now()
                    .checked_add(Duration::from_millis(CONNECTION_ID_VALID_DURATION_MILLIS as u64))
                    .unwrap();

                provider
                    .set_timeout(TimeoutToken::default(), next_timeout_at)
                    .expect("bip_utracker: Failed To Restart Connect Id Cleanup Timer");
            }
        };
    }
}

// ----------------------------------------------------------------------------//

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct TimeoutId {
    id: u128,
}

impl Default for TimeoutId {
    fn default() -> Self {
        Self {
            id: UNIX_EPOCH.elapsed().unwrap().as_nanos(),
        }
    }
}

impl TimeoutId {
    fn new(id: u128) -> Self {
        Self { id }
    }
}

impl Deref for TimeoutId {
    type Target = u128;

    fn deref(&self) -> &Self::Target {
        &self.id
    }
}

impl DerefMut for TimeoutId {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.id
    }
}

/// Contains logic for making sure a valid connection id is present
/// and correctly timing out when sending requests to the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct ConnectTimer {
    addr: SocketAddr,
    attempt: u64,
    request: ClientRequest,
    timeout_id: Option<TimeoutId>,
}

impl ConnectTimer {
    /// Create a new `ConnectTimer`.
    pub fn new(addr: SocketAddr, request: ClientRequest) -> ConnectTimer {
        ConnectTimer {
            addr,
            attempt: 0,
            request,
            timeout_id: None,
        }
    }

    /// Yields the current timeout value to use or None if the request should time out completely.
    #[instrument(skip(self), ret(level = Level::TRACE))]
    pub fn current_timeout(&mut self, timed_out: bool) -> Option<u64> {
        if self.attempt == MAXIMUM_REQUEST_RETRANSMIT_ATTEMPTS {
            tracing::warn!("request has reached maximum timeout attempts: {MAXIMUM_REQUEST_RETRANSMIT_ATTEMPTS}");

            None
        } else {
            if timed_out {
                self.attempt += 1;
            }

            Some(calculate_message_timeout_millis(self.attempt))
        }
    }

    /// Yields the current timeout id if one is set.
    pub fn timeout_id(&self) -> Option<TimeoutId> {
        self.timeout_id
    }

    /// Sets a new timeout id.
    pub fn set_timeout_id(&mut self, id: TimeoutId) {
        self.timeout_id = Some(id);
    }

    /// Yields the message parameters for the current connection.
    #[instrument(skip(self), ret(level = Level::TRACE))]
    pub fn message_params(&self) -> (SocketAddr, &ClientRequest) {
        (self.addr, &self.request)
    }
}

/// Calculates the timeout for the request given the attempt count.
#[instrument(skip())]
fn calculate_message_timeout_millis(attempt: u64) -> u64 {
    let attempt = attempt.try_into().unwrap_or(u32::MAX);
    let timeout = (15 * 2u64.pow(attempt)) * 1000;

    tracing::debug!(attempt, timeout, "calculated message timeout in milliseconds");

    timeout
}

// ----------------------------------------------------------------------------//

/// Cache for storing connection ids associated with a specific server address.
#[derive(Debug)]
struct ConnectIdCache {
    cache: HashMap<SocketAddr, (u64, Instant)>,
}

impl ConnectIdCache {
    /// Create a new connect id cache.
    fn new() -> ConnectIdCache {
        ConnectIdCache { cache: HashMap::new() }
    }

    /// Get an active connection id for the given addr.
    #[instrument(skip(self), ret(level = Level::TRACE))]
    fn get(&mut self, addr: SocketAddr) -> Option<u64> {
        match self.cache.entry(addr) {
            Entry::Vacant(_) => {
                tracing::debug!("connection id for {addr} not in cache");

                None
            }
            Entry::Occupied(occ) => {
                let curr_time = Instant::now();
                let prev_time = occ.get().1;

                if is_expired(curr_time, prev_time) {
                    occ.remove();

                    tracing::warn!("connection id was already expired");

                    None
                } else {
                    Some(occ.get().0)
                }
            }
        }
    }

    /// Put an un expired connection id into cache for the given addr.
    #[instrument(skip(self))]
    fn put(&mut self, addr: SocketAddr, connect_id: u64) {
        tracing::trace!("setting un expired connection id");

        let curr_time = Instant::now();

        self.cache.insert(addr, (connect_id, curr_time));
    }

    /// Removes all entries that have expired.
    #[instrument(skip(self))]
    fn clean_expired(&mut self) {
        let curr_time = Instant::now();
        let mut removed = 0;
        let mut curr_index = 0;

        let mut opt_curr_entry = self.cache.iter().skip(curr_index).map(|(&k, &v)| (k, v)).next();
        while let Some((addr, (_, prev_time))) = opt_curr_entry.take() {
            if is_expired(curr_time, prev_time) {
                self.cache.remove(&addr);
            }

            curr_index += 1;
            opt_curr_entry = self.cache.iter().skip(curr_index).map(|(&k, &v)| (k, v)).next();
        }

        if removed != 0 {
            tracing::debug!(%removed, "expired connection id(s)");
        }
    }
}

/// Returns true if the connect id received at `prev_time` is now expired.
#[instrument(skip(), ret(level = Level::TRACE))]
fn is_expired(curr_time: Instant, prev_time: Instant) -> bool {
    let Some(difference) = curr_time.checked_duration_since(prev_time) else {
        // in future
        return true;
    };

    difference >= Duration::from_millis(CONNECTION_ID_VALID_DURATION_MILLIS as u64)
}
