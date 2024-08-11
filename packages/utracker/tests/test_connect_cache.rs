use std::time::Duration;

use common::{tracing_stderr_init, MockTrackerHandler, DEFAULT_TIMEOUT, INIT};
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

    let server_addr = "127.0.0.1:3506".parse().unwrap();
    let mock_handler = MockTrackerHandler::new();
    let _server = TrackerServer::run(server_addr, mock_handler.clone()).unwrap();

    std::thread::sleep(Duration::from_millis(100));

    let mut client = TrackerClient::new("127.0.0.1:4506".parse().unwrap(), sink, None).unwrap();

    let first_hash = [0u8; bt::INFO_HASH_LEN].into();
    let second_hash = [1u8; bt::INFO_HASH_LEN].into();

    client.request(server_addr, ClientRequest::Scrape(first_hash)).unwrap();
    tokio::time::timeout(DEFAULT_TIMEOUT, stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert_eq!(mock_handler.num_active_connect_ids(), 1);

    for _ in 0..10 {
        client.request(server_addr, ClientRequest::Scrape(second_hash)).unwrap();
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
