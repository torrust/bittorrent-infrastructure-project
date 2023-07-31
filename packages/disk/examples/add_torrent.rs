extern crate disk;
extern crate futures;
extern crate metainfo;

use std::fs::File;
use std::io::{self, BufRead, Read, Write};

use disk::fs::NativeFileSystem;
use disk::{DiskManagerBuilder, IDiskMessage, ODiskMessage};
use futures::{Future, Sink, Stream};
use metainfo::Metainfo;

fn main() {
    println!("Utility For Allocating Disk Space For A Torrent File");

    let stdin = io::stdin();
    let mut input_lines = stdin.lock().lines();
    let mut stdout = io::stdout();

    print!("Enter the destination download directory: ");
    stdout.flush().unwrap();
    let download_path = input_lines.next().unwrap().unwrap();

    print!("Enter the full path to the torrent file: ");
    stdout.flush().unwrap();
    let torrent_path = input_lines.next().unwrap().unwrap();

    let mut torrent_bytes = Vec::new();
    File::open(torrent_path).unwrap().read_to_end(&mut torrent_bytes).unwrap();
    let metainfo_file = Metainfo::from_bytes(torrent_bytes).unwrap();

    let native_fs = NativeFileSystem::with_directory(download_path);
    let disk_manager = DiskManagerBuilder::new().build(native_fs);

    let (disk_send, disk_recv) = disk_manager.split();

    let total_pieces = metainfo_file.info().pieces().count();
    disk_send.send(IDiskMessage::AddTorrent(metainfo_file)).wait().unwrap();

    println!();

    let mut good_pieces = 0;
    for recv_msg in disk_recv.wait() {
        match recv_msg.unwrap() {
            ODiskMessage::TorrentAdded(hash) => {
                println!("Torrent With Hash {hash:?} Successfully Added");
                println!("Torrent Has {good_pieces} Good Pieces Out Of {total_pieces} Total Pieces");
                break;
            }
            ODiskMessage::FoundGoodPiece(_, _) => good_pieces += 1,
            unexpected => panic!("Unexpected ODiskMessage {unexpected:?}"),
        }
    }
}