use common::{handshaker, tracing_stderr_init, MockTrackerHandler, DEFAULT_TIMEOUT, INIT, LOOPBACK_IPV4};
use futures::StreamExt as _;
use tracing::level_filters::LevelFilter;
use util::bt::{self};
use utracker::{ClientRequest, HandshakerMessage, TrackerClient, TrackerServer};

mod common;

#[tokio::test]
async fn positive_scrape() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::ERROR);
    });

    let (sink, mut stream) = handshaker();

    let mock_handler = MockTrackerHandler::new();
    let server = TrackerServer::run(LOOPBACK_IPV4, mock_handler).unwrap();

    let mut client = TrackerClient::run(LOOPBACK_IPV4, sink, None).unwrap();

    let send_token = client
        .request(server.local_addr(), ClientRequest::Scrape([0u8; bt::INFO_HASH_LEN].into()))
        .unwrap();

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

    let response = metadata.result().as_ref().unwrap().scrape_response().unwrap();
    assert_eq!(response.iter().count(), 1);

    let stats = response.iter().next().unwrap();
    assert_eq!(stats.num_seeders(), 0);
    assert_eq!(stats.num_downloads(), 0);
    assert_eq!(stats.num_leechers(), 0);
}
