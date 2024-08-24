use common::{random_buffer, tracing_stderr_init, InMemoryFileSystem, MultiFileDirectAccessor, DEFAULT_TIMEOUT, INIT};
use disk::{DiskManagerBuilder, IDiskMessage};
use futures::{FutureExt, SinkExt as _, StreamExt as _};
use metainfo::{Metainfo, MetainfoBuilder, PieceLength};
use tracing::level_filters::LevelFilter;

mod common;

#[tokio::test]
async fn positive_disk_manager_send_backpressure() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::INFO);
    });

    // Create some "files" as random bytes
    let data_a = (random_buffer(50), "/path/to/file/a".into());
    let data_b = (random_buffer(2000), "/path/to/file/b".into());
    let data_c = (random_buffer(0), "/path/to/file/c".into());

    // Create our accessor for our in-memory files and create a torrent file for them
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
    let (mut m_send, mut m_recv) = DiskManagerBuilder::new()
        .with_sink_buffer_capacity(1)
        .build(filesystem.clone())
        .into_parts();

    // Add a torrent, so our receiver has a single torrent added message buffered
    tokio::time::timeout(DEFAULT_TIMEOUT, m_send.send(IDiskMessage::AddTorrent(metainfo_file)))
        .await
        .unwrap()
        .unwrap();

    // Try to send a remove message (but it should fail)
    assert!(
        m_send.send(IDiskMessage::RemoveTorrent(info_hash)).now_or_never().is_none(),
        "it should have back_pressure"
    );

    // Receive from our stream to unblock the backpressure
    tokio::time::timeout(DEFAULT_TIMEOUT, m_recv.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    // Try to send a remove message again which should go through
    tokio::time::timeout(DEFAULT_TIMEOUT, m_send.send(IDiskMessage::RemoveTorrent(info_hash)))
        .await
        .unwrap()
        .unwrap();

    // Receive confirmation
    tokio::time::timeout(DEFAULT_TIMEOUT, m_recv.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
