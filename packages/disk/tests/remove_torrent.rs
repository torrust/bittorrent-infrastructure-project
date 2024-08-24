use bytes::BytesMut;
use common::{
    random_buffer, runtime_loop_with_timeout, tracing_stderr_init, InMemoryFileSystem, MultiFileDirectAccessor, DEFAULT_TIMEOUT,
    INIT,
};
use disk::{Block, BlockMetadata, DiskManagerBuilder, IDiskMessage, ODiskMessage};
use futures::{future, FutureExt, SinkExt as _};
use metainfo::{Metainfo, MetainfoBuilder, PieceLength};
use tracing::level_filters::LevelFilter;

mod common;

#[tokio::test]
async fn positive_remove_torrent() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::INFO);
    });

    // Create some "files" as random bytes
    let data_a = (random_buffer(50), "/path/to/file/a".into());
    let data_b = (random_buffer(2000), "/path/to/file/b".into());
    let data_c = (random_buffer(0), "/path/to/file/c".into());

    // Create our accessor for our in memory files and create a torrent file for them
    let files_accessor =
        MultiFileDirectAccessor::new("/my/downloads/".into(), vec![data_a.clone(), data_b.clone(), data_c.clone()]);
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
    send.send(IDiskMessage::AddTorrent(metainfo_file)).await.unwrap();

    // Verify that zero pieces are marked as good
    let (mut send, good_pieces, recv) = runtime_loop_with_timeout(
        DEFAULT_TIMEOUT,
        ((send, 0), recv),
        |(mut send, good_pieces), recv, msg| match msg {
            Ok(ODiskMessage::TorrentAdded(_)) => {
                let fut = async move {
                    send.send(IDiskMessage::RemoveTorrent(info_hash)).await.unwrap();

                    ((send, good_pieces), recv)
                }
                .boxed();

                future::Either::Right(fut)
            }
            Ok(ODiskMessage::TorrentRemoved(_)) => future::Either::Left(future::ready((send, good_pieces, recv)).boxed()),
            Ok(ODiskMessage::FoundGoodPiece(_, _)) => {
                future::Either::Right(future::ready(((send, good_pieces + 1), recv)).boxed())
            }
            unexpected => panic!("Unexpected Message: {unexpected:?}"),
        },
    )
    .await;

    assert_eq!(0, good_pieces);

    let mut process_bytes = BytesMut::new();
    process_bytes.extend_from_slice(&data_a.0[0..50]);

    let process_block = Block::new(BlockMetadata::new(info_hash, 0, 0, 50), process_bytes.freeze());

    send.send(IDiskMessage::ProcessBlock(process_block)).await.unwrap();

    runtime_loop_with_timeout(DEFAULT_TIMEOUT, ((), recv), |(), _, msg| match msg {
        Ok(ODiskMessage::ProcessBlockError(_, _)) => future::Either::Left(future::ready(()).boxed()),
        unexpected => panic!("Unexpected Message: {unexpected:?}"),
    })
    .await;
}
