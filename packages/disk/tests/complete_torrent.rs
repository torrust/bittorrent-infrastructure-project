use common::{
    random_buffer, runtime_loop_with_timeout, send_block, tracing_stderr_init, InMemoryFileSystem, MultiFileDirectAccessor,
    DEFAULT_TIMEOUT, INIT,
};
use disk::{DiskManagerBuilder, IDiskMessage, ODiskMessage};
use futures::future::{self, Either};
use futures::{FutureExt, SinkExt as _};
use metainfo::{Metainfo, MetainfoBuilder, PieceLength};
use tokio::task::JoinSet;
use tracing::level_filters::LevelFilter;

mod common;

#[allow(unused_variables)]
#[allow(unreachable_code)]
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn positive_complete_torrent() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::INFO);
    });

    let mut tasks = JoinSet::new();

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

    // Spin up a disk manager and add our created torrent to it
    let filesystem = InMemoryFileSystem::new();
    let disk_manager = DiskManagerBuilder::new().build(filesystem.clone());

    let (mut send, recv) = disk_manager.into_parts();
    send.send(IDiskMessage::AddTorrent(metainfo_file.clone())).await.unwrap();

    // Verify that zero pieces are marked as good
    // Run a runtime loop until we get the TorrentAdded message
    let (good_pieces, recv) = runtime_loop_with_timeout(DEFAULT_TIMEOUT, (0, recv), |good_pieces, recv, msg| match msg {
        Ok(ODiskMessage::TorrentAdded(_)) => Either::Left(future::ready((good_pieces, recv)).boxed()),
        Ok(ODiskMessage::FoundGoodPiece(_, _)) => Either::Right(future::ready((good_pieces + 1, recv)).boxed()),
        unexpected => panic!("Unexpected Message: {unexpected:?}"),
    })
    .await;

    // Make sure we have no good pieces
    assert_eq!(0, good_pieces);

    let files_bytes = {
        let mut b = Vec::new();
        b.extend_from_slice(&data_a.0);
        b.extend_from_slice(&data_b.0);
        b
    };

    tracing::debug!("send two blocks that are known to be good, then one bad block");

    let send_one_bad_and_two_good = {
        let mut send = send.clone();
        let data = files_bytes.clone();
        let info_hash = metainfo_file.info().info_hash();

        async move {
            // Send piece 0 with a bad last block
            send_block(&mut send, &data[0..500], info_hash, 0, 0, 500, |_| ()).await;
            send_block(&mut send, &data[500..1000], info_hash, 0, 500, 500, |_| ()).await;
            send_block(&mut send, &data[1000..1024], info_hash, 0, 1000, 24, |bytes| {
                bytes[0] = !bytes[0];
            })
            .await;

            // Send piece 1 with good blocks
            send_block(&mut send, &data[1024..(1024 + 500)], info_hash, 1, 0, 500, |_| ()).await;
            send_block(&mut send, &data[(1024 + 500)..(1024 + 1000)], info_hash, 1, 500, 500, |_| ()).await;
            send_block(&mut send, &data[(1024 + 1000)..(1024 + 1024)], info_hash, 1, 1000, 24, |_| ()).await;

            // Send piece 2 with good blocks
            send_block(&mut send, &data[2048..(2048 + 500)], info_hash, 2, 0, 500, |_| ()).await;
            send_block(&mut send, &data[(2048 + 500)..(2048 + 975)], info_hash, 2, 500, 475, |_| ()).await;
        }
        .boxed()
    };

    tasks.spawn(send_one_bad_and_two_good);

    tracing::debug!("verify that piece 0 is bad, but piece 1 and 2 are good");

    let (recv, piece_zero_good, piece_one_good, piece_two_good) = runtime_loop_with_timeout(
        DEFAULT_TIMEOUT,
        ((false, false, false, 0), recv),
        |(piece_zero_good, piece_one_good, piece_two_good, messages_recvd), recv, msg| {
            let messages_recvd = messages_recvd + 1;

            // Map BlockProcessed to a None piece index so we don't update our state
            let (opt_piece_index, new_value) = match msg {
                Ok(ODiskMessage::FoundGoodPiece(_, index)) => (Some(index), true),
                Ok(ODiskMessage::FoundBadPiece(_, index)) => (Some(index), false),
                Ok(ODiskMessage::BlockProcessed(_)) => (None, false),
                unexpected => panic!("Unexpected Message: {unexpected:?}"),
            };

            let (piece_zero_good, piece_one_good, piece_two_good) = match opt_piece_index {
                None => (piece_zero_good, piece_one_good, piece_two_good),
                Some(0) => (new_value, piece_one_good, piece_two_good),
                Some(1) => (piece_zero_good, new_value, piece_two_good),
                Some(2) => (piece_zero_good, piece_one_good, new_value),
                Some(x) => panic!("Unexpected Index {x:?}"),
            };

            // One message for each block (8 blocks), plus 3 messages for bad/good
            if messages_recvd == (8 + 3) {
                Either::Left(future::ready((recv, piece_zero_good, piece_one_good, piece_two_good)).boxed())
            } else {
                Either::Right(future::ready(((piece_zero_good, piece_one_good, piece_two_good, messages_recvd), recv)).boxed())
            }
        },
    )
    .await;

    // Assert whether or not pieces were good
    assert!(!piece_zero_good);
    assert!(piece_one_good);
    assert!(piece_two_good);

    {
        tokio::task::yield_now().await;

        while let Some(task) = tasks.try_join_next() {
            match task {
                Ok(()) => continue,
                Err(e) => panic!("task joined with error: {e}"),
            }
        }

        assert!(tasks.is_empty(), "all the tasks should have finished now");
    }

    tracing::debug!("resend piece 0 with good blocks");

    let resend_with_good_blocks = {
        let mut send = send.clone();
        let data = files_bytes.clone();
        let info_hash = metainfo_file.info().info_hash();
        async move {
            send_block(&mut send, &data[0..500], info_hash, 0, 0, 500, |_| ()).await;
            send_block(&mut send, &data[500..1000], info_hash, 0, 500, 500, |_| ()).await;
            send_block(&mut send, &data[1000..1024], info_hash, 0, 1000, 24, |_| ()).await;
        }
        .boxed()
    };

    tasks.spawn(resend_with_good_blocks);

    tracing::debug!("verify that piece 0 is now good");

    let piece_zero_good = runtime_loop_with_timeout(
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

            // One message for each block (3 blocks), plus 1 messages for bad/good
            if messages_recvd == (3 + 1) {
                Either::Left(future::ready(piece_zero_good).boxed())
            } else {
                Either::Right(future::ready(((piece_zero_good, messages_recvd), recv)).boxed())
            }
        },
    )
    .await;

    {
        tokio::task::yield_now().await;

        while let Some(task) = tasks.try_join_next() {
            match task {
                Ok(()) => continue,
                Err(e) => panic!("task joined with error: {e}"),
            }
        }

        assert!(tasks.is_empty(), "all the tasks should have finished now");
    }

    // Assert whether or not piece was good
    assert!(piece_zero_good);
}
