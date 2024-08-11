use common::{handshaker, tracing_stderr_init, DEFAULT_TIMEOUT, INIT};
use futures::StreamExt as _;
use tracing::level_filters::LevelFilter;
use util::bt::{self};
use utracker::announce::{AnnounceEvent, ClientState};
use utracker::{ClientRequest, TrackerClient};

mod common;

#[tokio::test]
async fn positive_client_request_dropped() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::ERROR);
    });

    let (sink, stream) = handshaker();

    let server_addr = "127.0.0.1:3504".parse().unwrap();

    let request_capacity = 10;

    let mut client = TrackerClient::new("127.0.0.1:4504".parse().unwrap(), sink, Some(request_capacity)).unwrap();

    tracing::warn!("sending announce requests to fill buffer");
    for i in 1..=request_capacity {
        tracing::warn!("request {i} of {request_capacity}");

        client
            .request(
                server_addr,
                ClientRequest::Announce(
                    [0u8; bt::INFO_HASH_LEN].into(),
                    ClientState::new(0, 0, 0, AnnounceEvent::Started),
                ),
            )
            .unwrap();
    }

    tracing::warn!("sending one more announce request, it should fail");
    assert!(client
        .request(
            server_addr,
            ClientRequest::Announce(
                [0u8; bt::INFO_HASH_LEN].into(),
                ClientState::new(0, 0, 0, AnnounceEvent::Started)
            )
        )
        .is_none());

    std::mem::drop(client);

    let buffer: Vec<_> = tokio::time::timeout(DEFAULT_TIMEOUT, stream.collect()).await.unwrap();
    assert_eq!(request_capacity, buffer.len());
}
