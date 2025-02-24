use bytes::BytesMut;
use common::{
    random_buffer, runtime_loop_with_timeout, tracing_stderr_init, InMemoryFileSystem, MultiFileDirectAccessor, DEFAULT_TIMEOUT,
    INIT,
};
use disk::{Block, BlockMetadata, DiskManagerBuilder, FileSystem, IDiskMessage, ODiskMessage};
use futures::{future, FutureExt as _, SinkExt as _};
use metainfo::{Metainfo, MetainfoBuilder, PieceLength};
use tracing::level_filters::LevelFilter;

mod common;

#[tokio::test]
async fn positive_process_block() {
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

    // Spin up a disk manager and add our created torrent to its
    let filesystem = InMemoryFileSystem::new();
    let disk_manager = DiskManagerBuilder::new().build(filesystem.clone());

    let mut process_bytes = BytesMut::new();
    process_bytes.extend_from_slice(&data_b.0[1..=50]);

    let process_block = Block::new(
        BlockMetadata::new(metainfo_file.info().info_hash(), 1, 0, 50),
        process_bytes.freeze(),
    );

    let (mut send, recv) = disk_manager.into_parts();
    send.send(IDiskMessage::AddTorrent(metainfo_file)).await.unwrap();

    runtime_loop_with_timeout(
        DEFAULT_TIMEOUT,
        ((send, Some(process_block)), recv),
        |(mut send, opt_pblock), recv, msg| match msg {
            Ok(ODiskMessage::TorrentAdded(_)) => {
                let fut = async move {
                    send.send(IDiskMessage::ProcessBlock(opt_pblock.unwrap())).await.unwrap();

                    ((send, None), recv)
                }
                .boxed();

                future::Either::Right(fut)
            }
            Ok(ODiskMessage::BlockProcessed(_)) => future::Either::Left(future::ready(()).boxed()),
            unexpected => panic!("Unexpected Message: {unexpected:?}"),
        },
    )
    .await;

    // Verify block was updated in data_b
    let mut received_file_b = filesystem.open_file(data_b.1).unwrap();
    assert_eq!(2000, filesystem.file_size(&received_file_b).unwrap());

    let mut received_file_b_data = vec![0u8; 2000];
    assert_eq!(
        2000,
        filesystem
            .read_file(&mut received_file_b, 0, &mut received_file_b_data)
            .unwrap()
    );

    let mut expected_file_b_data = vec![0u8; 2000];
    expected_file_b_data[1..=50].copy_from_slice(&data_b.0[1..=50]);
    assert_eq!(expected_file_b_data, received_file_b_data);
}
