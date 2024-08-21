use common::{tracing_stderr_init, MockTrackerHandler, DEFAULT_TIMEOUT, INIT, LOOPBACK_IPV4};
use futures::StreamExt as _;
use tracing::level_filters::LevelFilter;
use util::bt::{self};
use utracker::{ClientRequest, TrackerClient, TrackerServer};

mod common;

#[tokio::test]
async fn positive_connection_id_cache() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::ERROR);
    });

    let (sink, mut stream) = common::handshaker();

    let mock_handler = MockTrackerHandler::new();
    let server = TrackerServer::run(LOOPBACK_IPV4, mock_handler.clone()).unwrap();

    let mut client = TrackerClient::run(LOOPBACK_IPV4, sink, None).unwrap();

    let first_hash = [0u8; bt::INFO_HASH_LEN].into();
    let second_hash = [1u8; bt::INFO_HASH_LEN].into();

    client
        .request(server.local_addr(), ClientRequest::Scrape(first_hash))
        .unwrap();
    tokio::time::timeout(DEFAULT_TIMEOUT, stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert_eq!(mock_handler.num_active_connect_ids(), 1);

    for _ in 0..10 {
        client
            .request(server.local_addr(), ClientRequest::Scrape(second_hash))
            .unwrap();
    }

    for _ in 0..10 {
        tokio::time::timeout(DEFAULT_TIMEOUT, stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    assert_eq!(mock_handler.num_active_connect_ids(), 1);
}
