use bytes::BytesMut;
use common::{random_buffer, tracing_stderr_init, InMemoryFileSystem, MultiFileDirectAccessor, INIT};
use disk::{Block, BlockMetadata, BlockMut, DiskManagerBuilder, IDiskMessage, ODiskMessage};
use futures::{SinkExt as _, StreamExt as _};
use metainfo::{Metainfo, MetainfoBuilder, PieceLength};
use tokio::time::{timeout, Duration};
use tracing::level_filters::LevelFilter;

mod common;

#[tokio::test]
async fn positive_load_block() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::INFO);
    });

    // Create some "files" as random bytes
    let data_a = (random_buffer(1023), "/path/to/file/a".into());
    let data_b = (random_buffer(2000), "/path/to/file/b".into());

    // Create our accessor for our in-memory files and create a torrent file for them
    let files_accessor = MultiFileDirectAccessor::new("/my/downloads/".into(), vec![data_a.clone(), data_b.clone()]);
    let metainfo_bytes = MetainfoBuilder::new()
        .set_piece_length(PieceLength::Custom(1024))
        .build(1, files_accessor, |_| ())
        .unwrap();
    let metainfo_file = Metainfo::from_bytes(metainfo_bytes).unwrap();

    // Spin up a disk manager and add our created torrent to it
    let filesystem = InMemoryFileSystem::new();
    let disk_manager = DiskManagerBuilder::new().build(filesystem.clone());

    let mut process_block = BytesMut::new();
    process_block.extend_from_slice(&data_b.0[1..=50]);

    let mut load_block = BytesMut::with_capacity(50);
    load_block.extend_from_slice(&[0u8; 50]);

    let process_block = Block::new(
        BlockMetadata::new(metainfo_file.info().info_hash(), 1, 0, 50),
        process_block.freeze(),
    );
    let load_block = BlockMut::new(BlockMetadata::new(metainfo_file.info().info_hash(), 1, 0, 50), load_block);

    let (mut send, mut recv) = disk_manager.into_parts();
    send.send(IDiskMessage::AddTorrent(metainfo_file)).await.unwrap();

    let timeout_duration = Duration::from_millis(500);
    let result = timeout(timeout_duration, async {
        loop {
            match recv.next().await {
                Some(Ok(ODiskMessage::TorrentAdded(_))) => {
                    send.send(IDiskMessage::ProcessBlock(process_block.clone())).await.unwrap();
                }
                Some(Ok(ODiskMessage::BlockProcessed(_block))) => {
                    send.send(IDiskMessage::LoadBlock(load_block.clone())).await.unwrap();
                }
                Some(Ok(ODiskMessage::BlockLoaded(block))) => {
                    return (process_block, block);
                }
                Some(unexpected) => panic!("Unexpected Message: {unexpected:?}"),
                None => panic!("End Of Stream Reached"),
            }
        }
    })
    .await;

    let (pblock, lblock) = result.unwrap();

    // Verify lblock contains our data
    assert_eq!(*pblock, *lblock);
}
