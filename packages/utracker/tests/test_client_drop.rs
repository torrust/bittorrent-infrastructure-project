use std::net::SocketAddr;

use common::{handshaker, tracing_stderr_init, DEFAULT_TIMEOUT, INIT};
use futures::StreamExt as _;
use tracing::level_filters::LevelFilter;
use util::bt::{self};
use utracker::announce::{AnnounceEvent, ClientState};
use utracker::{ClientError, ClientRequest, HandshakerMessage, TrackerClient};

mod common;

#[tokio::test]
async fn positive_client_request_failed() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::ERROR);
    });

    let (sink, mut stream) = handshaker();

    let server_addr: SocketAddr = "127.0.0.1:3503".parse().unwrap();
    // Don't actually create the server since we want the request to wait for a little bit until we drop

    let send_token = {
        let mut client = TrackerClient::new("127.0.0.1:4503".parse().unwrap(), sink, None).unwrap();

        client
            .request(
                server_addr,
                ClientRequest::Announce(
                    [0u8; bt::INFO_HASH_LEN].into(),
                    ClientState::new(0, 0, 0, AnnounceEvent::None),
                ),
            )
            .unwrap()
    };
    // Client is now dropped

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

    match metadata.result() {
        Err(ClientError::ClientShutdown) => (),
        _ => panic!("Did Not Receive ClientShutdown..."),
    }
}
