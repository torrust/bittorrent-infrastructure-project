use std::time::Duration;

use common::{tracing_stderr_init, INIT};
use futures::{SinkExt as _, StreamExt as _};
use handshake::Extensions;
use metainfo::{DirectAccessor, Metainfo, MetainfoBuilder, PieceLength};
use peer::PeerInfo;
use select::revelation::error::RevealError;
use select::revelation::{HonestRevealModuleBuilder, IRevealMessage, ORevealMessage};
use select::ControlMessage;
use tracing::level_filters::LevelFilter;
use util::bt;
use util::bt::InfoHash;

mod common;

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

#[tokio::test]
async fn positive_add_and_remove_metainfo() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::INFO);
    });

    let builder = HonestRevealModuleBuilder::new();
    let mut module = builder.build();
    let metainfo = metainfo(1);

    module
        .send(IRevealMessage::Control(ControlMessage::AddTorrent(metainfo.clone())))
        .await
        .unwrap();
    module
        .send(IRevealMessage::Control(ControlMessage::RemoveTorrent(metainfo.clone())))
        .await
        .unwrap();
}

#[tokio::test]
async fn positive_send_bitfield_single_piece() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::INFO);
    });

    let builder = HonestRevealModuleBuilder::new();
    let mut module = builder.build();
    let metainfo = metainfo(8);
    let info_hash = metainfo.info().info_hash();
    let peer_info = peer_info(info_hash);

    tracing::debug!("sending add torrent...");
    module
        .send(IRevealMessage::Control(ControlMessage::AddTorrent(metainfo)))
        .await
        .unwrap();

    tracing::debug!("sending found good piece...");
    module.send(IRevealMessage::FoundGoodPiece(info_hash, 0)).await.unwrap();

    tracing::debug!("sending peer connected...");
    module
        .send(IRevealMessage::Control(ControlMessage::PeerConnected(peer_info)))
        .await
        .unwrap();

    tracing::debug!("receiving send bit field...");
    let message = module.next().await.unwrap();
    if let Ok(ORevealMessage::SendBitField(info, bitfield)) = message {
        assert_eq!(peer_info, info);
        assert_eq!(1, bitfield.bitfield().len());
        assert_eq!(0x80, bitfield.bitfield()[0]);
    } else {
        panic!("Received Unexpected Message");
    }
}

#[tokio::test]
async fn positive_send_bitfield_multiple_pieces() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::INFO);
    });

    let builder = HonestRevealModuleBuilder::new();
    let mut module = builder.build();
    let metainfo = metainfo(16);
    let info_hash = metainfo.info().info_hash();
    let peer_info = peer_info(info_hash);

    module
        .send(IRevealMessage::Control(ControlMessage::AddTorrent(metainfo)))
        .await
        .unwrap();
    module.send(IRevealMessage::FoundGoodPiece(info_hash, 0)).await.unwrap();
    module.send(IRevealMessage::FoundGoodPiece(info_hash, 8)).await.unwrap();
    module.send(IRevealMessage::FoundGoodPiece(info_hash, 15)).await.unwrap();
    module
        .send(IRevealMessage::Control(ControlMessage::PeerConnected(peer_info)))
        .await
        .unwrap();

    let message = module.next().await.unwrap();
    if let Ok(ORevealMessage::SendBitField(info, bitfield)) = message {
        assert_eq!(peer_info, info);
        assert_eq!(2, bitfield.bitfield().len());
        assert_eq!(0x80, bitfield.bitfield()[0]);
        assert_eq!(0x81, bitfield.bitfield()[1]);
    } else {
        panic!("Received Unexpected Message");
    }
}

#[tokio::test]
async fn negative_do_not_send_empty_bitfield() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::INFO);
    });

    let builder = HonestRevealModuleBuilder::new();
    let mut module = builder.build();
    let metainfo = metainfo(16);
    let info_hash = metainfo.info().info_hash();
    let peer_info = peer_info(info_hash);

    tracing::debug!("sending add torrent...");
    module
        .send(IRevealMessage::Control(ControlMessage::AddTorrent(metainfo)))
        .await
        .unwrap();

    tracing::debug!("sending peer connected...");
    module
        .send(IRevealMessage::Control(ControlMessage::PeerConnected(peer_info)))
        .await
        .unwrap();

    tracing::debug!("attempt to receive a message...");
    let res = tokio::time::timeout(Duration::from_millis(50), module.next()).await;

    if let Ok(item) = res {
        panic!("expected timeout, but got a result: {item:?}");
    } else {
        tracing::debug!("timeout was reached");
    }
}

#[tokio::test]
async fn negative_found_good_piece_out_of_range() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::INFO);
    });

    let builder = HonestRevealModuleBuilder::new();
    let mut module = builder.build();
    let metainfo = metainfo(8);
    let info_hash = metainfo.info().info_hash();

    module
        .send(IRevealMessage::Control(ControlMessage::AddTorrent(metainfo)))
        .await
        .unwrap();

    let error = module.send(IRevealMessage::FoundGoodPiece(info_hash, 8)).await.unwrap_err();
    match error {
        RevealError::InvalidPieceOutOfRange { hash, index } => {
            assert_eq!(info_hash, hash);
            assert_eq!(8, index);
        }
        _ => {
            panic!("Received Unexpected Error: {error:?}");
        }
    }
}

#[tokio::test]
async fn negative_all_pieces_good_found_good_piece_out_of_range() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::INFO);
    });

    let builder = HonestRevealModuleBuilder::new();
    let mut module = builder.build();
    let metainfo = metainfo(3);
    let info_hash = metainfo.info().info_hash();

    module
        .send(IRevealMessage::Control(ControlMessage::AddTorrent(metainfo)))
        .await
        .unwrap();
    module.send(IRevealMessage::FoundGoodPiece(info_hash, 0)).await.unwrap();
    module.send(IRevealMessage::FoundGoodPiece(info_hash, 1)).await.unwrap();
    module.send(IRevealMessage::FoundGoodPiece(info_hash, 2)).await.unwrap();

    let error = module.send(IRevealMessage::FoundGoodPiece(info_hash, 3)).await.unwrap_err();
    match error {
        RevealError::InvalidPieceOutOfRange { hash, index } => {
            assert_eq!(info_hash, hash);
            assert_eq!(3, index);
        }
        _ => {
            panic!("Received Unexpected Error: {error:?}");
        }
    }
}
