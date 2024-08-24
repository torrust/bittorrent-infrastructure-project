use std::io::{BufRead, Read as _, Write as _};
use std::sync::{Arc, Once};

use disk::fs::NativeFileSystem;
use disk::{DiskManagerBuilder, IDiskMessage, ODiskMessage};
use futures::{SinkExt, StreamExt};
use metainfo::Metainfo;
use tracing::level_filters::LevelFilter;

static INIT: Once = Once::new();

fn tracing_stderr_init(filter: LevelFilter) {
    let builder = tracing_subscriber::fmt().with_max_level(filter).with_ansi(true);

    builder.pretty().with_file(true).init();

    tracing::info!("Logging initialized");
}

#[tokio::main]
async fn main() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::INFO);
    });

    tracing::info!("Utility For Allocating Disk Space For A Torrent File");

    let stdin = std::io::stdin();
    let mut input_lines = stdin.lock().lines();
    let mut stdout = std::io::stdout();

    print!("Enter the destination download directory: ");
    stdout.flush().unwrap();
    let download_path = input_lines.next().unwrap().unwrap();

    print!("Enter the full path to the torrent file: ");
    stdout.flush().unwrap();
    let torrent_path = input_lines.next().unwrap().unwrap();

    let mut torrent_bytes = Vec::new();
    std::fs::File::open(torrent_path)
        .unwrap()
        .read_to_end(&mut torrent_bytes)
        .unwrap();
    let metainfo_file = Metainfo::from_bytes(torrent_bytes).unwrap();

    let filesystem = NativeFileSystem::with_directory(download_path);
    let disk_manager = DiskManagerBuilder::new().build(Arc::new(filesystem));

    let (mut disk_send, mut disk_recv) = disk_manager.into_parts();

    let total_pieces = metainfo_file.info().pieces().count();
    disk_send.send(IDiskMessage::AddTorrent(metainfo_file)).await.unwrap();

    let mut good_pieces = 0;
    while let Some(recv_msg) = disk_recv.next().await {
        match recv_msg.unwrap() {
            ODiskMessage::TorrentAdded(hash) => {
                tracing::info!("Torrent With Hash {hash:?} Successfully Added");
                tracing::info!("Torrent Has {good_pieces} Good Pieces Out Of {total_pieces} Total Pieces");
                break;
            }
            ODiskMessage::FoundGoodPiece(_, _) => good_pieces += 1,
            unexpected => panic!("Unexpected ODiskMessage {unexpected:?}"),
        }
    }
}
