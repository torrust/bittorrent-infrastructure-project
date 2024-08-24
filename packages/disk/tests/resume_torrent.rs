use common::{
    random_buffer, runtime_loop_with_timeout, send_block, tracing_stderr_init, InMemoryFileSystem, MultiFileDirectAccessor,
    DEFAULT_TIMEOUT, INIT,
};
use disk::{DiskManagerBuilder, IDiskMessage, ODiskMessage};
use futures::{future, FutureExt, SinkExt as _};
use metainfo::{Metainfo, MetainfoBuilder, PieceLength};
use tracing::level_filters::LevelFilter;

mod common;

#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn positive_complete_torrent() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::INFO);
    });

    // Create some "files" as random bytes
    let data_a = (random_buffer(1023), "/path/to/file/a".into());
    let data_b = (random_buffer(2000), "/path/to/file/b".into());

    // Create our accessor for our in memory files and create a torrent file for them
    let files_accessor = MultiFileDirectAccessor::new("/my/downloads/".into(), vec![data_a.clone(), data_b.clone()]);
    let metainfo_bytes = MetainfoBuilder::new()
        .set_piece_length(PieceLength::Custom(1024))
        .build(1, files_accessor, |_| ())
        .unwrap();
    let metainfo_file = Metainfo::from_bytes(metainfo_bytes).unwrap();
    let info_hash = metainfo_file.info().info_hash();

    // Spin up a disk manager and add our created torrent to it
    let filesystem = InMemoryFileSystem::new();
    let disk_manager = DiskManagerBuilder::new().build(filesystem.clone());

    let (mut send, recv) = disk_manager.into_parts();
    send.send(IDiskMessage::AddTorrent(metainfo_file.clone())).await.unwrap();

    // Verify that zero pieces are marked as good

    // Run a runtime loop until we get the TorrentAdded message
    let (good_pieces, recv) = runtime_loop_with_timeout(DEFAULT_TIMEOUT, (0, recv), |good_pieces, recv, msg| match msg {
        Ok(ODiskMessage::TorrentAdded(_)) => future::Either::Left(future::ready((good_pieces, recv)).boxed()),
        Ok(ODiskMessage::FoundGoodPiece(_, _)) => future::Either::Right(future::ready((good_pieces + 1, recv)).boxed()),
        unexpected => panic!("Unexpected Message: {unexpected:?}"),
    })
    .await;

    // Make sure we have no good pieces
    assert_eq!(0, good_pieces);

    // Send a couple blocks that are known to be good, then one bad block
    let mut files_bytes = Vec::new();
    files_bytes.extend_from_slice(&data_a.0);
    files_bytes.extend_from_slice(&data_b.0);

    // Send piece 0
    send_block(
        &mut send,
        &files_bytes[0..500],
        metainfo_file.info().info_hash(),
        0,
        0,
        500,
        |_| (),
    )
    .await;
    send_block(
        &mut send,
        &files_bytes[500..1000],
        metainfo_file.info().info_hash(),
        0,
        500,
        500,
        |_| (),
    )
    .await;
    send_block(
        &mut send,
        &files_bytes[1000..1024],
        metainfo_file.info().info_hash(),
        0,
        1000,
        24,
        |_| (),
    )
    .await;

    // Verify that piece 0 is good
    let (recv, piece_zero_good) = runtime_loop_with_timeout(
        DEFAULT_TIMEOUT,
        ((false, 0), recv),
        |(piece_zero_good, messages_recvd), recv, msg| {
            let messages_recvd = messages_recvd + 1;

            // Map BlockProcessed to a None piece index so we don't update our state
            let (opt_piece_index, new_value) = match msg {
                Ok(ODiskMessage::FoundGoodPiece(_, index)) => (Some(index), true),
                Ok(ODiskMessage::FoundBadPiece(_, index)) => (Some(index), false),
                Ok(ODiskMessage::BlockProcessed(_)) => (None, false),
                unexpected => panic!("Unexpected Message: {unexpected:?}"),
            };

            let piece_zero_good = match opt_piece_index {
                None => piece_zero_good,
                Some(0) => new_value,
                Some(x) => panic!("Unexpected Index {x:?}"),
            };

            if messages_recvd == (3 + 1) {
                future::Either::Left(future::ready((recv, piece_zero_good)).boxed())
            } else {
                future::Either::Right(future::ready(((piece_zero_good, messages_recvd), recv)).boxed())
            }
        },
    )
    .await;

    // Assert whether or not pieces were good
    assert!(piece_zero_good);

    // Remove the torrent from our manager
    send.send(IDiskMessage::RemoveTorrent(info_hash)).await.unwrap();

    // Verify that our torrent was removed
    let recv = runtime_loop_with_timeout(DEFAULT_TIMEOUT, ((), recv), |(), recv, msg| match msg {
        Ok(ODiskMessage::TorrentRemoved(_)) => future::Either::Left(future::ready(recv).boxed()),
        unexpected => panic!("Unexpected Message: {unexpected:?}"),
    })
    .await;

    // Re-add our torrent and verify that we see our good first block
    send.send(IDiskMessage::AddTorrent(metainfo_file.clone())).await.unwrap();

    let (recv, piece_zero_good) =
        runtime_loop_with_timeout(DEFAULT_TIMEOUT, (false, recv), |piece_zero_good, recv, msg| match msg {
            Ok(ODiskMessage::TorrentAdded(_)) => future::Either::Left(future::ready((recv, piece_zero_good)).boxed()),
            Ok(ODiskMessage::FoundGoodPiece(_, 0)) => future::Either::Right(future::ready((true, recv)).boxed()),
            unexpected => panic!("Unexpected Message: {unexpected:?}"),
        })
        .await;

    assert!(piece_zero_good);

    // Send piece 1
    send_block(
        &mut send,
        &files_bytes[1024..(1024 + 500)],
        metainfo_file.info().info_hash(),
        1,
        0,
        500,
        |_| (),
    )
    .await;
    send_block(
        &mut send,
        &files_bytes[(1024 + 500)..(1024 + 1000)],
        metainfo_file.info().info_hash(),
        1,
        500,
        500,
        |_| (),
    )
    .await;
    send_block(
        &mut send,
        &files_bytes[(1024 + 1000)..(1024 + 1024)],
        metainfo_file.info().info_hash(),
        1,
        1000,
        24,
        |_| (),
    )
    .await;

    // Send piece 2
    send_block(
        &mut send,
        &files_bytes[2048..(2048 + 500)],
        metainfo_file.info().info_hash(),
        2,
        0,
        500,
        |_| (),
    )
    .await;
    send_block(
        &mut send,
        &files_bytes[(2048 + 500)..(2048 + 975)],
        metainfo_file.info().info_hash(),
        2,
        500,
        475,
        |_| (),
    )
    .await;

    // Verify last two blocks are good
    let (piece_one_good, piece_two_good) = runtime_loop_with_timeout(
        DEFAULT_TIMEOUT,
        ((false, false, 0), recv),
        |(piece_one_good, piece_two_good, messages_recvd), recv, msg| {
            let messages_recvd = messages_recvd + 1;

            // Map BlockProcessed to a None piece index so we don't update our state
            let (opt_piece_index, new_value) = match msg {
                Ok(ODiskMessage::FoundGoodPiece(_, index)) => (Some(index), true),
                Ok(ODiskMessage::FoundBadPiece(_, index)) => (Some(index), false),
                Ok(ODiskMessage::BlockProcessed(_)) => (None, false),
                unexpected => panic!("Unexpected Message: {unexpected:?}"),
            };

            let (piece_one_good, piece_two_good) = match opt_piece_index {
                None => (piece_one_good, piece_two_good),
                Some(1) => (new_value, piece_two_good),
                Some(2) => (piece_one_good, new_value),
                Some(x) => panic!("Unexpected Index {x:?}"),
            };

            if messages_recvd == (5 + 2) {
                future::Either::Left(future::ready((piece_one_good, piece_two_good)).boxed())
            } else {
                future::Either::Right(future::ready(((piece_one_good, piece_two_good, messages_recvd), recv)).boxed())
            }
        },
    )
    .await;

    assert!(piece_one_good);
    assert!(piece_two_good);
}
