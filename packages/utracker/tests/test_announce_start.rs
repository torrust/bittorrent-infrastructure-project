use std::net::SocketAddr;
use std::time::Duration;

use common::{handshaker, tracing_stderr_init, MockTrackerHandler, DEFAULT_TIMEOUT, INIT};
use futures::StreamExt as _;
use handshake::Protocol;
use tracing::level_filters::LevelFilter;
use util::bt::{self};
use utracker::announce::{AnnounceEvent, ClientState};
use utracker::{ClientRequest, HandshakerMessage, TrackerClient, TrackerServer};

mod common;

#[tokio::test]
async fn positive_announce_started() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::ERROR);
    });

    let (handshaker_sender, mut handshaker_receiver) = handshaker();

    let server_addr = "127.0.0.1:3501".parse().unwrap();
    let mock_handler = MockTrackerHandler::new();
    let _server = TrackerServer::run(server_addr, mock_handler).unwrap();

    std::thread::sleep(Duration::from_millis(100));

    let mut client = TrackerClient::new("127.0.0.1:4501".parse().unwrap(), handshaker_sender, None).unwrap();

    let hash = [0u8; bt::INFO_HASH_LEN].into();

    tracing::warn!("sending announce");
    let _send_token = client
        .request(
            server_addr,
            ClientRequest::Announce(hash, ClientState::new(0, 0, 0, AnnounceEvent::Started)),
        )
        .unwrap();

    tracing::warn!("receiving initiate message");
    let init_msg = match tokio::time::timeout(DEFAULT_TIMEOUT, handshaker_receiver.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
    {
        HandshakerMessage::InitiateMessage(message) => message,
        HandshakerMessage::ClientMetadata(_) => unreachable!(),
    };

    let exp_peer_addr: SocketAddr = "127.0.0.1:6969".parse().unwrap();

    assert_eq!(&Protocol::BitTorrent, init_msg.protocol());
    assert_eq!(&exp_peer_addr, init_msg.address());
    assert_eq!(&hash, init_msg.hash());

    tracing::warn!("receiving client metadata");
    let metadata = match tokio::time::timeout(DEFAULT_TIMEOUT, handshaker_receiver.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
    {
        HandshakerMessage::InitiateMessage(_) => unreachable!(),
        HandshakerMessage::ClientMetadata(metadata) => metadata,
    };
    let metadata_result = metadata.result().as_ref().unwrap().announce_response().unwrap();

    assert_eq!(metadata_result.leechers(), 1);
    assert_eq!(metadata_result.seeders(), 1);
    assert_eq!(metadata_result.peers().iter().count(), 1);
}
