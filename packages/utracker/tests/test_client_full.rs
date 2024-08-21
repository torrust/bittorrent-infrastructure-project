use std::net::SocketAddr;

use common::{handshaker, tracing_stderr_init, DEFAULT_TIMEOUT, INIT, LOOPBACK_IPV4};
use futures::StreamExt as _;
use tokio::net::UdpSocket;
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

    // We bind a temp socket, then drop it...
    let disconnected_addr: SocketAddr = {
        let socket = UdpSocket::bind(LOOPBACK_IPV4).await.unwrap();
        socket.local_addr().unwrap()
    };

    tokio::task::yield_now().await;

    let request_capacity = 10;

    {
        let mut client = TrackerClient::run(LOOPBACK_IPV4, sink, Some(request_capacity)).unwrap();

        tracing::warn!("sending announce requests to fill buffer");
        for i in 1..=request_capacity {
            tracing::warn!("request {i} of {request_capacity}");

            client
                .request(
                    disconnected_addr,
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
                disconnected_addr,
                ClientRequest::Announce(
                    [0u8; bt::INFO_HASH_LEN].into(),
                    ClientState::new(0, 0, 0, AnnounceEvent::Started)
                )
            )
            .is_none());
    }
    // Client is now dropped

    tracing::warn!("collecting remaining messages");
    let buffer: Vec<_> = tokio::time::timeout(DEFAULT_TIMEOUT, stream.collect()).await.unwrap();
    assert_eq!(request_capacity, buffer.len());
}
