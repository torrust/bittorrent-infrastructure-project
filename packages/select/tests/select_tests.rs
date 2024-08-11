use futures::{Async, Sink, Stream};
use futures_test::harness::Harness;
use handshake::Extensions;
use metainfo::{DirectAccessor, Metainfo, MetainfoBuilder, PieceLength};
use peer::PeerInfo;
use select::revelation::error::RevealErrorKind;
use select::revelation::{HonestRevealModule, IRevealMessage, ORevealMessage};
use select::ControlMessage;
use util::bt;
use util::bt::InfoHash;

fn metainfo(num_pieces: usize) -> Metainfo {
    let data = vec![0u8; num_pieces];

    let accessor = DirectAccessor::new("MyFile.txt", &data);
    let bytes = MetainfoBuilder::new()
        .set_piece_length(PieceLength::Custom(1))
        .build(1, accessor, |_| ())
        .unwrap();

    Metainfo::from_bytes(bytes).unwrap()
}

fn peer_info(hash: InfoHash) -> PeerInfo {
    PeerInfo::new(
        "0.0.0.0:0".parse().unwrap(),
        [0u8; bt::PEER_ID_LEN].into(),
        hash,
        Extensions::new(),
    )
}

#[test]
fn positive_add_and_remove_metainfo() {
    let (send, _recv) = HonestRevealModule::new().split();
    let metainfo = metainfo(1);

    let mut block_send = send.wait();

    block_send
        .send(IRevealMessage::Control(ControlMessage::AddTorrent(metainfo.clone())))
        .unwrap();
    block_send
        .send(IRevealMessage::Control(ControlMessage::RemoveTorrent(metainfo.clone())))
        .unwrap();
}

#[test]
fn positive_send_bitfield_single_piece() {
    let (send, recv) = HonestRevealModule::new().split();
    let metainfo = metainfo(8);
    let info_hash = metainfo.info().info_hash();
    let peer_info = peer_info(info_hash);

    let mut block_send = send.wait();
    let mut block_recv = recv.wait();

    block_send
        .send(IRevealMessage::Control(ControlMessage::AddTorrent(metainfo)))
        .unwrap();
    block_send.send(IRevealMessage::FoundGoodPiece(info_hash, 0)).unwrap();
    block_send
        .send(IRevealMessage::Control(ControlMessage::PeerConnected(peer_info)))
        .unwrap();

    let ORevealMessage::SendBitField(info, bitfield) = block_recv.next().unwrap().unwrap() else {
        panic!("Received Unexpected Message")
    };

    assert_eq!(peer_info, info);
    assert_eq!(1, bitfield.bitfield().len());
    assert_eq!(0x80, bitfield.bitfield()[0]);
}

#[test]
fn positive_send_bitfield_multiple_pieces() {
    let (send, recv) = HonestRevealModule::new().split();
    let metainfo = metainfo(16);
    let info_hash = metainfo.info().info_hash();
    let peer_info = peer_info(info_hash);

    let mut block_send = send.wait();
    let mut block_recv = recv.wait();

    block_send
        .send(IRevealMessage::Control(ControlMessage::AddTorrent(metainfo)))
        .unwrap();
    block_send.send(IRevealMessage::FoundGoodPiece(info_hash, 0)).unwrap();
    block_send.send(IRevealMessage::FoundGoodPiece(info_hash, 8)).unwrap();
    block_send.send(IRevealMessage::FoundGoodPiece(info_hash, 15)).unwrap();
    block_send
        .send(IRevealMessage::Control(ControlMessage::PeerConnected(peer_info)))
        .unwrap();

    let ORevealMessage::SendBitField(info, bitfield) = block_recv.next().unwrap().unwrap() else {
        panic!("Received Unexpected Message")
    };

    assert_eq!(peer_info, info);
    assert_eq!(2, bitfield.bitfield().len());
    assert_eq!(0x80, bitfield.bitfield()[0]);
    assert_eq!(0x81, bitfield.bitfield()[1]);
}

#[test]
fn negative_do_not_send_empty_bitfield() {
    let (send, recv) = HonestRevealModule::new().split();
    let metainfo = metainfo(16);
    let info_hash = metainfo.info().info_hash();
    let peer_info = peer_info(info_hash);

    let mut block_send = send.wait();
    let mut non_block_recv = Harness::new(recv);

    block_send
        .send(IRevealMessage::Control(ControlMessage::AddTorrent(metainfo)))
        .unwrap();
    block_send
        .send(IRevealMessage::Control(ControlMessage::PeerConnected(peer_info)))
        .unwrap();

    assert!(non_block_recv.poll_next().as_ref().map(Async::is_not_ready).unwrap_or(false));
}

#[test]
fn negative_found_good_piece_out_of_range() {
    let (send, _recv) = HonestRevealModule::new().split();
    let metainfo = metainfo(8);
    let info_hash = metainfo.info().info_hash();

    let mut block_send = send.wait();

    block_send
        .send(IRevealMessage::Control(ControlMessage::AddTorrent(metainfo)))
        .unwrap();

    let error = block_send.send(IRevealMessage::FoundGoodPiece(info_hash, 8)).unwrap_err();
    match error.kind() {
        &RevealErrorKind::InvalidPieceOutOfRange { hash, index } => {
            assert_eq!(info_hash, hash);
            assert_eq!(8, index);
        }
        _ => {
            panic!("Received Unexpected Message")
        }
    };
}

#[test]
fn negative_all_pieces_good_found_good_piece_out_of_range() {
    let (send, _recv) = HonestRevealModule::new().split();
    let metainfo = metainfo(3);
    let info_hash = metainfo.info().info_hash();

    let mut block_send = send.wait();

    block_send
        .send(IRevealMessage::Control(ControlMessage::AddTorrent(metainfo)))
        .unwrap();
    block_send.send(IRevealMessage::FoundGoodPiece(info_hash, 0)).unwrap();
    block_send.send(IRevealMessage::FoundGoodPiece(info_hash, 1)).unwrap();
    block_send.send(IRevealMessage::FoundGoodPiece(info_hash, 2)).unwrap();

    let error = block_send.send(IRevealMessage::FoundGoodPiece(info_hash, 3)).unwrap_err();
    match error.kind() {
        &RevealErrorKind::InvalidPieceOutOfRange { hash, index } => {
            assert_eq!(info_hash, hash);
            assert_eq!(3, index);
        }
        _ => {
            panic!("Received Unexpected Message")
        }
    };
}
