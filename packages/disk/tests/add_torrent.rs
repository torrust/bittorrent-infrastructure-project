use common::{
    random_buffer, runtime_loop_with_timeout, tracing_stderr_init, InMemoryFileSystem, MultiFileDirectAccessor, DEFAULT_TIMEOUT,
    INIT,
};
use disk::{DiskManagerBuilder, FileSystem as _, IDiskMessage, ODiskMessage};
use futures::future::{self, Either};
use futures::{FutureExt, SinkExt as _};
use metainfo::{Metainfo, MetainfoBuilder, PieceLength};
use tracing::level_filters::LevelFilter;

mod common;

#[tokio::test]
async fn positive_add_torrent() {
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

    // Spin up a disk manager and add our created torrent to it
    let filesystem = InMemoryFileSystem::new();
    let disk_manager = DiskManagerBuilder::new().build(filesystem.me());

    let (mut send, recv) = disk_manager.into_parts();
    send.send(IDiskMessage::AddTorrent(metainfo_file)).await.unwrap();

    // Verify that zero pieces are marked as good
    // Run a runtime loop until we get the TorrentAdded message
    let good_pieces = runtime_loop_with_timeout(DEFAULT_TIMEOUT, (0, recv), |good_pieces, recv, msg| match msg {
        Ok(ODiskMessage::TorrentAdded(_)) => Either::Left(future::ready(good_pieces).boxed()),
        Ok(ODiskMessage::FoundGoodPiece(_, _)) => Either::Right(future::ready((good_pieces + 1, recv)).boxed()),
        unexpected => panic!("Unexpected Message: {unexpected:?}"),
    })
    .await;

    assert_eq!(0, good_pieces);

    // Verify file a in file system
    let mut received_file_a = filesystem.open_file(data_a.1).unwrap();
    assert_eq!(50, filesystem.file_size(&received_file_a).unwrap());

    let mut received_buffer_a = vec![0u8; 50];
    assert_eq!(
        50,
        filesystem
            .read_file(&mut received_file_a, 0, &mut received_buffer_a[..])
            .unwrap()
    );
    assert_eq!(vec![0u8; 50], received_buffer_a);

    // Verify file b in file system
    let mut received_file_b = filesystem.open_file(data_b.1).unwrap();
    assert_eq!(2000, filesystem.file_size(&received_file_b).unwrap());

    let mut received_buffer_b = vec![0u8; 2000];
    assert_eq!(
        2000,
        filesystem
            .read_file(&mut received_file_b, 0, &mut received_buffer_b[..])
            .unwrap()
    );
    assert_eq!(vec![0u8; 2000], received_buffer_b);

    // Verify file c in file system
    let mut received_file_c = filesystem.open_file(data_c.1).unwrap();
    assert_eq!(0, filesystem.file_size(&received_file_c).unwrap());

    let mut received_buffer_c = vec![0u8; 0];
    assert_eq!(
        0,
        filesystem
            .read_file(&mut received_file_c, 0, &mut received_buffer_c[..])
            .unwrap()
    );
    assert_eq!(vec![0u8; 0], received_buffer_c);
}
