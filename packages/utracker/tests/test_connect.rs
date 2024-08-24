use common::{handshaker, tracing_stderr_init, MockTrackerHandler, DEFAULT_TIMEOUT, INIT, LOOPBACK_IPV4};
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

    let mock_handler = MockTrackerHandler::new();
    let server = TrackerServer::run(LOOPBACK_IPV4, mock_handler).unwrap();

    let mut client = TrackerClient::run(LOOPBACK_IPV4, sink, None).unwrap();

    let send_token = client
        .request(
            server.local_addr(),
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
