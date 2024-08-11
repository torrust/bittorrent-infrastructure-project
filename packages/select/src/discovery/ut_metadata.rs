use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Write as _;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use bytes::BytesMut;
use futures::sink::Sink;
use futures::stream::Stream;
use handshake::InfoHash;
use metainfo::{Info, Metainfo};
use peer::messages::builders::ExtendedMessageBuilder;
use peer::messages::{
    ExtendedMessage, ExtendedType, UtMetadataDataMessage, UtMetadataMessage, UtMetadataRejectMessage, UtMetadataRequestMessage,
};
use peer::PeerInfo;
use rand::{self, Rng};

use crate::discovery::error::DiscoveryError;
use crate::discovery::{IDiscoveryMessage, ODiscoveryMessage};
use crate::extended::{ExtendedListener, ExtendedPeerInfo};
use crate::ControlMessage;

const REQUEST_TIMEOUT_MILLIS: u64 = 2000;
const MAX_REQUEST_SIZE: usize = 16 * 1024;
const MAX_ACTIVE_REQUESTS: usize = 100;
const MAX_PEER_REQUESTS: usize = 100;

struct PendingInfo {
    messages: Vec<UtMetadataRequestMessage>,
    left: usize,
    bytes: Vec<u8>,
}

struct ActiveRequest {
    left: Duration,
    message: UtMetadataRequestMessage,
    sent_to: PeerInfo,
}

struct PeerRequest {
    send_to: PeerInfo,
    request: UtMetadataRequestMessage,
}

struct ActivePeers {
    peers: HashSet<PeerInfo>,
    metadata_size: i64,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Default)]
pub struct UtMetadataModule {
    completed_map: HashMap<InfoHash, Vec<u8>>,
    pending_map: HashMap<InfoHash, Option<PendingInfo>>,
    active_peers: HashMap<InfoHash, ActivePeers>,
    active_requests: Vec<ActiveRequest>,
    peer_requests: VecDeque<PeerRequest>,
    opt_sink_waker: Option<Waker>,
    opt_stream_waker: Option<Waker>,
}

impl UtMetadataModule {
    #[must_use]
    pub fn new() -> UtMetadataModule {
        UtMetadataModule {
            completed_map: HashMap::new(),
            pending_map: HashMap::new(),
            active_peers: HashMap::new(),
            active_requests: Vec::new(),
            peer_requests: VecDeque::new(),
            opt_sink_waker: None,
            opt_stream_waker: None,
        }
    }

    fn add_torrent(&mut self, metainfo: &Metainfo) -> Result<(), DiscoveryError> {
        let info_hash = metainfo.info().info_hash();
        match self.completed_map.entry(info_hash) {
            Entry::Occupied(_) => Err(DiscoveryError::InvalidMetainfoExists { hash: info_hash }),
            Entry::Vacant(vac) => {
                let info_bytes = metainfo.info().to_bytes();
                vac.insert(info_bytes);
                Ok(())
            }
        }
    }

    fn remove_torrent(&mut self, metainfo: &Metainfo) -> Result<(), DiscoveryError> {
        if self.completed_map.remove(&metainfo.info().info_hash()).is_none() {
            Err(DiscoveryError::InvalidMetainfoNotExists {
                hash: metainfo.info().info_hash(),
            })
        } else {
            Ok(())
        }
    }

    fn add_peer(&mut self, info: PeerInfo, ext_info: &ExtendedPeerInfo) {
        let our_support = ext_info
            .our_message()
            .and_then(|msg| msg.query_id(&ExtendedType::UtMetadata))
            .is_some();
        let they_support = ext_info
            .their_message()
            .and_then(|msg| msg.query_id(&ExtendedType::UtMetadata))
            .is_some();
        let opt_metadata_size = ext_info.their_message().and_then(ExtendedMessage::metadata_size);

        tracing::info!(
            "Our Support For UtMetadata Is {:?} And {:?} Support For UtMetadata Is {:?} With Metadata Size {:?}",
            our_support,
            info.addr(),
            they_support,
            opt_metadata_size
        );

        if let (true, true, Some(metadata_size)) = (our_support, they_support, opt_metadata_size) {
            self.active_peers
                .entry(*info.hash())
                .or_insert_with(|| ActivePeers {
                    peers: HashSet::new(),
                    metadata_size,
                })
                .peers
                .insert(info);
        }
    }

    fn remove_peer(&mut self, info: PeerInfo) {
        if let Some(active_peers) = self.active_peers.get_mut(info.hash()) {
            active_peers.peers.remove(&info);
            if active_peers.peers.is_empty() {
                self.active_peers.remove(info.hash());
            }
        }
    }

    fn apply_tick(&mut self, duration: Duration) {
        self.active_requests.retain(|request| {
            let is_expired = request.left.checked_sub(duration).is_none();
            if is_expired {
                if let Some(active) = self.active_peers.get_mut(request.sent_to.hash()) {
                    active.peers.remove(&request.sent_to);
                }
                if let Some(Some(pending)) = self.pending_map.get_mut(request.sent_to.hash()) {
                    pending.messages.push(request.message);
                }
            }
            !is_expired
        });

        for active_request in &mut self.active_requests {
            active_request.left -= duration;
        }
    }

    fn download_metainfo(&mut self, hash: InfoHash) {
        self.pending_map.entry(hash).or_insert(None);
    }

    fn recv_request(&mut self, info: PeerInfo, request: UtMetadataRequestMessage) {
        if self.peer_requests.len() < MAX_PEER_REQUESTS {
            self.peer_requests.push_back(PeerRequest { send_to: info, request });
        }
    }

    fn recv_data(&mut self, info: PeerInfo, data: &UtMetadataDataMessage) {
        if let Some(index) = self
            .active_requests
            .iter()
            .position(|request| request.sent_to == info && request.message.piece() == data.piece())
        {
            self.active_requests.swap_remove(index);
            if let Some(Some(pending)) = self.pending_map.get_mut(info.hash()) {
                let piece: usize = data.piece().try_into().unwrap();
                let data_offset = piece.checked_mul(MAX_REQUEST_SIZE).unwrap();
                pending.left -= 1;
                (&mut pending.bytes.as_mut_slice()[data_offset..])
                    .write_all(data.data().as_ref())
                    .unwrap();
            }
        }
    }

    fn recv_reject(_info: PeerInfo, _reject: UtMetadataRejectMessage) {
        // TODO: Remove any requests after receiving a reject, for now, we will just timeout
    }

    fn retrieve_completed_download(&mut self) -> Option<Result<ODiscoveryMessage, DiscoveryError>> {
        let opt_completed_hash = self
            .pending_map
            .iter()
            .find(|(_, opt_pending)| opt_pending.as_ref().map_or(false, |pending| pending.left == 0))
            .map(|(hash, _)| *hash);

        opt_completed_hash.and_then(|completed_hash| {
            let completed = self.pending_map.remove(&completed_hash).unwrap().unwrap();
            self.active_peers.remove(&completed_hash);
            match Info::from_bytes(&completed.bytes[..]) {
                Ok(info) => Some(Ok(ODiscoveryMessage::DownloadedMetainfo(info.into()))),
                Err(_) => self.retrieve_completed_download(),
            }
        })
    }

    fn retrieve_piece_request(&mut self) -> Option<Result<ODiscoveryMessage, DiscoveryError>> {
        for (hash, opt_pending) in &mut self.pending_map {
            if let Some(pending) = opt_pending {
                if !pending.messages.is_empty() {
                    if let Some(active_peers) = self.active_peers.get(hash) {
                        if !active_peers.peers.is_empty() {
                            let mut active_peers_iter = active_peers.peers.iter();
                            let num_active_peers = active_peers_iter.len();
                            let selected_peer_num = rand::thread_rng().gen::<usize>() % num_active_peers;
                            let selected_peer = active_peers_iter.nth(selected_peer_num).unwrap();
                            let selected_message = pending.messages.pop().unwrap();
                            self.active_requests
                                .push(generate_active_request(selected_message, *selected_peer));
                            tracing::info!(
                                "Requesting Piece {:?} For Hash {:?}",
                                selected_message.piece(),
                                selected_peer.hash()
                            );
                            return Some(Ok(ODiscoveryMessage::SendUtMetadataMessage(
                                *selected_peer,
                                UtMetadataMessage::Request(selected_message),
                            )));
                        }
                    }
                }
            }
        }
        None
    }

    fn retrieve_piece_response(&mut self) -> Option<Result<ODiscoveryMessage, DiscoveryError>> {
        while let Some(request) = self.peer_requests.pop_front() {
            let hash = request.send_to.hash();
            let piece: usize = request.request.piece().try_into().unwrap();
            let start = piece * MAX_REQUEST_SIZE;
            let end = start + MAX_REQUEST_SIZE;
            if let Some(data) = self.completed_map.get(hash) {
                if start <= data.len() && end <= data.len() {
                    let info_slice = &data[start..end];
                    let mut info_payload = BytesMut::with_capacity(info_slice.len());
                    info_payload.extend_from_slice(info_slice);
                    let message = UtMetadataDataMessage::new(
                        piece.try_into().unwrap(),
                        info_slice.len().try_into().unwrap(),
                        info_payload.freeze(),
                    );
                    return Some(Ok(ODiscoveryMessage::SendUtMetadataMessage(
                        request.send_to,
                        UtMetadataMessage::Data(message),
                    )));
                }
            }
        }
        None
    }

    fn initialize_pending(&mut self) -> bool {
        let mut pending_tasks_available = false;
        for (hash, opt_pending) in &mut self.pending_map {
            if opt_pending.is_none() {
                if let Some(active_peers) = self.active_peers.get(hash) {
                    *opt_pending = Some(pending_info_from_metadata_size(active_peers.metadata_size));
                }
            }
            pending_tasks_available |= opt_pending.as_ref().map_or(false, |pending| !pending.messages.is_empty());
        }
        pending_tasks_available
    }

    fn validate_downloaded(&mut self) -> bool {
        let mut completed_downloads_available = false;
        for (&expected_hash, opt_pending) in &mut self.pending_map {
            if let Some(pending) = opt_pending {
                if pending.left == 0 {
                    let real_hash = InfoHash::from_bytes(&pending.bytes[..]);
                    if real_hash == expected_hash {
                        completed_downloads_available = true;
                    } else {
                        *opt_pending = None;
                    }
                }
            }
        }
        completed_downloads_available
    }

    fn check_stream_unblock(&mut self) {
        let downloads_available = self.validate_downloaded();
        let tasks_available = self.initialize_pending();
        let free_task_queue_space = self.active_requests.len() != MAX_ACTIVE_REQUESTS;
        let peer_requests_available = !self.peer_requests.is_empty();
        let should_unblock = self.opt_stream_waker.is_some()
            && ((free_task_queue_space && tasks_available) || peer_requests_available || downloads_available);
        if should_unblock {
            if let Some(waker) = self.opt_stream_waker.take() {
                waker.wake();
            }
        }
    }

    fn check_sink_unblock(&mut self) {
        let should_unblock = self.opt_sink_waker.is_some() && self.peer_requests.len() != MAX_PEER_REQUESTS;
        if should_unblock {
            if let Some(waker) = self.opt_sink_waker.take() {
                waker.wake();
            }
        }
    }
}

fn generate_active_request(message: UtMetadataRequestMessage, peer: PeerInfo) -> ActiveRequest {
    ActiveRequest {
        left: Duration::from_millis(REQUEST_TIMEOUT_MILLIS),
        message,
        sent_to: peer,
    }
}

fn pending_info_from_metadata_size(metadata_size: i64) -> PendingInfo {
    let cast_metadata_size: usize = metadata_size.try_into().unwrap();
    let bytes = vec![0u8; cast_metadata_size];
    let mut messages = Vec::new();
    let num_pieces = if cast_metadata_size % MAX_REQUEST_SIZE != 0 {
        cast_metadata_size / MAX_REQUEST_SIZE + 1
    } else {
        cast_metadata_size / MAX_REQUEST_SIZE
    };
    for index in 0..num_pieces {
        messages.push(UtMetadataRequestMessage::new(index.try_into().unwrap()));
    }
    PendingInfo {
        messages,
        left: num_pieces,
        bytes,
    }
}

impl ExtendedListener for UtMetadataModule {
    fn extend(&self, _info: &PeerInfo, builder: ExtendedMessageBuilder) -> ExtendedMessageBuilder {
        builder.with_extended_type(ExtendedType::UtMetadata, Some(5))
    }

    fn on_update(&mut self, info: &PeerInfo, extended: &ExtendedPeerInfo) {
        self.add_peer(*info, extended);
        self.check_stream_unblock();
    }
}

impl Sink<IDiscoveryMessage> for UtMetadataModule {
    type Error = DiscoveryError;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.peer_requests.len() < MAX_PEER_REQUESTS {
            Poll::Ready(Ok(()))
        } else {
            self.opt_sink_waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }

    fn start_send(mut self: Pin<&mut Self>, item: IDiscoveryMessage) -> Result<(), Self::Error> {
        match item {
            IDiscoveryMessage::Control(ControlMessage::AddTorrent(metainfo)) => self.add_torrent(&metainfo),
            IDiscoveryMessage::Control(ControlMessage::RemoveTorrent(metainfo)) => self.remove_torrent(&metainfo),
            IDiscoveryMessage::Control(ControlMessage::PeerConnected(_)) => Ok(()),
            IDiscoveryMessage::Control(ControlMessage::PeerDisconnected(info)) => {
                self.remove_peer(info);
                Ok(())
            }
            IDiscoveryMessage::Control(ControlMessage::Tick(duration)) => {
                self.apply_tick(duration);
                Ok(())
            }
            IDiscoveryMessage::DownloadMetainfo(hash) => {
                self.download_metainfo(hash);
                Ok(())
            }
            IDiscoveryMessage::ReceivedUtMetadataMessage(info, UtMetadataMessage::Request(msg)) => {
                self.recv_request(info, msg);
                Ok(())
            }
            IDiscoveryMessage::ReceivedUtMetadataMessage(info, UtMetadataMessage::Data(msg)) => {
                self.recv_data(info, &msg);
                Ok(())
            }
            IDiscoveryMessage::ReceivedUtMetadataMessage(info, UtMetadataMessage::Reject(msg)) => {
                UtMetadataModule::recv_reject(info, msg);
                Ok(())
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

impl Stream for UtMetadataModule {
    type Item = Result<ODiscoveryMessage, DiscoveryError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let opt_result = self
            .retrieve_completed_download()
            .or_else(|| self.retrieve_piece_request())
            .or_else(|| self.retrieve_piece_response());

        self.check_sink_unblock();

        if let Some(result) = opt_result {
            Poll::Ready(Some(result))
        } else {
            self.opt_stream_waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
