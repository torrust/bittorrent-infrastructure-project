use common::{tracing_stderr_init, INIT};
use futures::future::try_join;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use handshake::transports::TcpTransport;
use handshake::{DiscoveryInfo, HandshakerBuilder, InitiateMessage, Protocol};
use tokio::net::TcpStream;
use tracing::level_filters::LevelFilter;
use util::bt::{self};

mod common;

#[tokio::test]
async fn positive_connect() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::INFO);
    });

    let handshaker_one_addr = "127.0.0.1:0".parse().unwrap();
    let handshaker_one_pid = [4u8; bt::PEER_ID_LEN].into();

    let (mut handshaker_one, mut tasks_one) = HandshakerBuilder::new()
        .with_bind_addr(handshaker_one_addr)
        .with_peer_id(handshaker_one_pid)
        .build(TcpTransport)
        .await
        .unwrap();

    let mut handshaker_one_addr = handshaker_one_addr;
    handshaker_one_addr.set_port(handshaker_one.port());

    let handshaker_two_addr = "127.0.0.1:0".parse().unwrap();
    let handshaker_two_pid = [5u8; bt::PEER_ID_LEN].into();

    let (mut handshaker_two, mut tasks_two) = HandshakerBuilder::new()
        .with_bind_addr(handshaker_two_addr)
        .with_peer_id(handshaker_two_pid)
        .build(TcpTransport)
        .await
        .unwrap();

    let test = tokio::spawn(async move {
        let mut handshaker_two_addr = handshaker_two_addr;
        handshaker_two_addr.set_port(handshaker_two.port());

        // Send the initiate message first
        handshaker_one
            .send(InitiateMessage::new(
                Protocol::BitTorrent,
                [55u8; bt::INFO_HASH_LEN].into(),
                handshaker_two_addr,
            ))
            .await
            .unwrap();

        let handshaker_one_future = async {
            let message: handshake::CompleteMessage<TcpStream> = handshaker_one.next().await.unwrap().unwrap();
            Ok::<_, ()>(message)
        };

        let handshaker_two_future = async {
            let message: handshake::CompleteMessage<TcpStream> = handshaker_two.next().await.unwrap().unwrap();
            Ok::<_, ()>(message)
        };

        let (item_one, item_two) = try_join(handshaker_one_future, handshaker_two_future).await.unwrap();

        // Result from handshaker one should match handshaker two's listen address
        assert_eq!(handshaker_two_addr, *item_one.address());

        assert_eq!(handshaker_one_pid, *item_two.peer_id());
        assert_eq!(handshaker_two_pid, *item_one.peer_id());
    });

    let res = test.await;

    tasks_one.shutdown().await;
    tasks_two.shutdown().await;

    res.unwrap();
}
