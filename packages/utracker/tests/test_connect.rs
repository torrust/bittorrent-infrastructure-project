use std::time::Duration;

use common::{handshaker, tracing_stderr_init, MockTrackerHandler, DEFAULT_TIMEOUT, INIT};
use futures::StreamExt as _;
use tracing::level_filters::LevelFilter;
use util::bt::{self};
use utracker::announce::{AnnounceEvent, ClientState};
use utracker::{ClientRequest, HandshakerMessage, TrackerClient, TrackerServer};

mod common;

#[tokio::test]
async fn positive_receive_connect_id() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::ERROR);
    });

    let (sink, mut stream) = handshaker();

    let server_addr = "127.0.0.1:3505".parse().unwrap();
    let mock_handler = MockTrackerHandler::new();
    let _server = TrackerServer::run(server_addr, mock_handler).unwrap();

    std::thread::sleep(Duration::from_millis(100));

    let mut client = TrackerClient::new("127.0.0.1:4505".parse().unwrap(), sink, None).unwrap();

    let send_token = client
        .request(
            server_addr,
            ClientRequest::Announce(
                [0u8; bt::INFO_HASH_LEN].into(),
                ClientState::new(0, 0, 0, AnnounceEvent::None),
            ),
        )
        .unwrap();

    let _init_msg = match tokio::time::timeout(DEFAULT_TIMEOUT, stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
    {
        HandshakerMessage::InitiateMessage(message) => message,
        HandshakerMessage::ClientMetadata(_) => unreachable!(),
    };

    let metadata = match tokio::time::timeout(DEFAULT_TIMEOUT, stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
    {
        HandshakerMessage::InitiateMessage(_) => unreachable!(),
        HandshakerMessage::ClientMetadata(metadata) => metadata,
    };

    assert_eq!(send_token, metadata.token());
    assert!(metadata.result().is_ok());
}
