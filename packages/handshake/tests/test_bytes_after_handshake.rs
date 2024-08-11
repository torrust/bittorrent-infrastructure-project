use std::io::{Read as _, Write as _};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};

use common::{tracing_stderr_init, INIT};
use futures::stream::StreamExt;
use handshake::transports::TcpTransport;
use handshake::{DiscoveryInfo, HandshakerBuilder};
use tokio::io::AsyncReadExt as _;
use tracing::level_filters::LevelFilter;
use util::bt::{self};

mod common;

#[tokio::test]
async fn positive_recover_bytes() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::INFO);
    });

    let mut handshaker_one_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);

    let handshaker_one_pid = [4u8; bt::PEER_ID_LEN].into();

    let (mut handshaker_one, mut tasks_one) = HandshakerBuilder::new()
        .with_bind_addr(handshaker_one_addr)
        .with_peer_id(handshaker_one_pid)
        .build(TcpTransport)
        .await
        .unwrap();

    handshaker_one_addr.set_port(handshaker_one.port());

    tasks_one.spawn_blocking(move || {
        let mut stream = TcpStream::connect(handshaker_one_addr).unwrap();

        let mut write_buffer = Vec::new();

        write_buffer.write_all(&[1, 1]).unwrap();
        write_buffer.write_all(&[0u8; 8]).unwrap();
        write_buffer.write_all(&[0u8; bt::INFO_HASH_LEN]).unwrap();
        write_buffer.write_all(&[0u8; bt::PEER_ID_LEN]).unwrap();
        let expect_read_length = write_buffer.len();
        write_buffer.write_all(&[55u8; 100]).unwrap();

        stream.write_all(&write_buffer).unwrap();

        stream.read_exact(&mut vec![0u8; expect_read_length][..]).unwrap();
    });

    let test = tokio::spawn(async move {
        if let Some(message) = handshaker_one.next().await {
            let (_, _, _, _, _, mut sock) = message.unwrap().into_parts();

            let mut recv_buffer = vec![0u8; 100];
            sock.read_exact(&mut recv_buffer).await.unwrap();

            // Assert that our buffer contains the bytes after the handshake
            assert_eq!(vec![55u8; 100], recv_buffer);
        } else {
            panic!("Failed to receive handshake message");
        }
    });

    let res = test.await;

    tasks_one.shutdown().await;

    res.unwrap();
}
