use std::net::SocketAddr;

use common::{handshaker, tracing_stderr_init, DEFAULT_TIMEOUT, INIT, LOOPBACK_IPV4};
use futures::StreamExt as _;
use tokio::net::UdpSocket;
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

    let (sink, stream) = handshaker();

    // We bind a temp socket, then drop it...
    let disconnected_addr: SocketAddr = {
        let socket = UdpSocket::bind(LOOPBACK_IPV4).await.unwrap();
        socket.local_addr().unwrap()
    };

    tokio::task::yield_now().await;

    let send_token = {
        let mut client = TrackerClient::run(LOOPBACK_IPV4, sink, None).unwrap();

        client
            .request(
                disconnected_addr,
                ClientRequest::Announce(
                    [0u8; bt::INFO_HASH_LEN].into(),
                    ClientState::new(0, 0, 0, AnnounceEvent::None),
                ),
            )
            .unwrap()
    };
    // Client is now dropped

    let mut messages: Vec<_> = tokio::time::timeout(DEFAULT_TIMEOUT, stream.collect())
        .await
        .expect("it should not time out");

    while let Some(message) = messages.pop() {
        let metadata = match message.expect("it should be a handshake message") {
            HandshakerMessage::InitiateMessage(_) => unreachable!(),
            HandshakerMessage::ClientMetadata(metadata) => metadata,
        };

        assert_eq!(send_token, metadata.token());

        match metadata.result() {
            Err(ClientError::ClientShutdown) => (),
            _ => panic!("Did Not Receive ClientShutdown..."),
        }
    }
}
