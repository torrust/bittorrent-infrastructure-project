use common::connected_channel::{connected_channel, ConnectedChannel};
use common::{add_peer, remove_peer, tracing_stderr_init, INIT};
use futures::SinkExt as _;
use handshake::Extensions;
use peer::error::PeerManagerError;
use peer::messages::PeerWireProtocolMessage;
use peer::protocols::NullProtocol;
use peer::{PeerInfo, PeerManagerBuilder, PeerManagerInputMessage};
use tracing::level_filters::LevelFilter;
use util::bt;

mod common;

type Peer = ConnectedChannel<
    Result<PeerWireProtocolMessage<NullProtocol>, std::io::Error>,
    Result<PeerWireProtocolMessage<NullProtocol>, std::io::Error>,
>;

#[tokio::test]
async fn positive_peer_manager_send_backpressure() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::INFO);
    });

    let (mut send, mut recv) = PeerManagerBuilder::new()
        .with_peer_capacity(1)
        .build::<Peer, PeerWireProtocolMessage<NullProtocol>>()
        .into_parts();

    // Create two peers
    let (peer_one, peer_two): (Peer, Peer) = connected_channel(5);
    let peer_one_info = create_peer_info("127.0.0.1:0", [0u8; bt::PEER_ID_LEN], [0u8; bt::INFO_HASH_LEN]);
    let peer_two_info = create_peer_info("127.0.0.1:1", [1u8; bt::PEER_ID_LEN], [1u8; bt::INFO_HASH_LEN]);

    // Add peer one to the manager
    add_peer(&mut send, &mut recv, peer_one_info, peer_one).await.unwrap();

    // Try to add peer two, but make sure it was denied (start send returned not ready)
    let Err(full) = send.start_send_unpin(Ok(PeerManagerInputMessage::AddPeer(peer_two_info, peer_two.clone()))) else {
        panic!("it should not add to full peer store")
    };

    let PeerManagerError::PeerStoreFull(capacity) = full else {
        panic!("it should be a peer store full error, but got: {full:?}")
    };

    assert_eq!(capacity, 1);

    // Remove peer one from the manager
    remove_peer(&mut send, &mut recv, peer_one_info).await.unwrap();

    // Try to add peer two, but make sure it goes through
    add_peer(&mut send, &mut recv, peer_two_info, peer_two).await.unwrap();
}

fn create_peer_info(addr: &str, peer_id: [u8; bt::PEER_ID_LEN], info_hash: [u8; bt::INFO_HASH_LEN]) -> PeerInfo {
    PeerInfo::new(
        addr.parse().expect("Invalid address"),
        peer_id.into(),
        info_hash.into(),
        Extensions::new(),
    )
}
