use std::collections::HashMap;
use std::io::Read as _;
use std::sync::{Arc, Once};

use disk::fs::NativeFileSystem;
use disk::fs_cache::FileHandleCache;
use disk::{
    Block, BlockMetadata, BlockMut, DiskManager, DiskManagerBuilder, DiskManagerSink, DiskManagerStream, IDiskMessage, InfoHash,
    ODiskMessage,
};
use futures::channel::mpsc;
use futures::future::Either;
use futures::lock::Mutex;
use futures::{stream, SinkExt as _, StreamExt as _};
use handshake::transports::TcpTransport;
use handshake::{
    Extensions, Handshaker, HandshakerBuilder, HandshakerConfig, HandshakerStream, InitiateMessage, PeerId, Protocol,
};
use metainfo::{Info, Metainfo};
use peer::messages::{BitFieldMessage, HaveMessage, PeerWireProtocolMessage, PieceMessage, RequestMessage};
use peer::protocols::{NullProtocol, PeerWireProtocol};
use peer::{
    PeerInfo, PeerManagerBuilder, PeerManagerInputMessage, PeerManagerOutputError, PeerManagerOutputMessage, PeerManagerSink,
    PeerManagerStream, PeerProtocolCodec,
};
use tokio::net::TcpStream;
use tokio::signal;
use tokio::task::JoinSet;
use tokio_util::bytes::BytesMut;
use tokio_util::codec::{Decoder, Framed};
use tracing::level_filters::LevelFilter;

// Maximum number of requests that can be in flight at once.
const MAX_PENDING_BLOCKS: usize = 50;

pub static INIT: Once = Once::new();

// Enum to store our selection state updates
#[allow(dead_code)]
#[derive(Debug)]
enum PeerSelectionState {
    Choke(PeerInfo),
    UnChoke(PeerInfo),
    Interested(PeerInfo),
    UnInterested(PeerInfo),
    Have(PeerInfo, HaveMessage),
    BitField(PeerInfo, BitFieldMessage),
    NewPeer(PeerInfo),
    RemovedPeer(PeerInfo),
    BlockProcessed,
    GoodPiece(u64),
    BadPiece(u64),
    TorrentSynced,
    TorrentAdded,
}

enum Downloader {
    Finished,
    Interrupted,
}

enum Setup {
    Finished((NativeDiskManager, PeerManager, TcpHandshaker), JoinSet<()>),
    Interrupted,
}

pub fn tracing_stdout_init(filter: LevelFilter) {
    let builder = tracing_subscriber::fmt()
        .with_max_level(filter)
        .with_ansi(true)
        .with_writer(std::io::stdout);

    builder.pretty().with_file(true).init();

    tracing::info!("Logging initialized");
}

async fn ctrl_c() {
    signal::ctrl_c().await.expect("failed to listen for event");
    tracing::warn!("Ctrl-C received, shutting down...");
}

#[tokio::main]
async fn main() {
    INIT.call_once(|| {
        tracing_stdout_init(LevelFilter::TRACE);
    });

    // Parse command-line arguments
    let matched_arguments = parse_arguments();
    let (torrent_file_path, download_directory, peer_address) = extract_arguments(&matched_arguments);

    // Load and parse the torrent file
    let (metainfo, info_hash) = load_and_parse_torrent_file(&torrent_file_path);

    // Create a JoinSet to manage background tasks
    let tasks = Arc::new(Mutex::new(JoinSet::new()));

    // Setup the managers.
    let setup = setup(download_directory);

    // Await either the completion of the setup or the Ctrl-C signal
    let setup = tokio::select! {
        setup = setup => Setup::Finished(setup.0, setup.1),
        () = ctrl_c() => Setup::Interrupted,
    };

    let (managers, mut handshaker_tasks) = match setup {
        Setup::Finished(managers, handshaker_tasks) => (managers, handshaker_tasks),
        Setup::Interrupted => {
            tracing::warn!("setup was canceled...");
            return;
        }
    };

    let downloader = downloader(tasks.clone(), managers, peer_address, metainfo, info_hash);

    // Await either the completion of the downloader or the Ctrl-C signal
    let status = tokio::select! {
        () = downloader => Downloader::Finished,
        () = ctrl_c() => Downloader::Interrupted,
    };

    match status {
        Downloader::Finished => {
            while let Some(result) = handshaker_tasks.try_join_next() {
                if let Err(e) = result {
                    eprintln!("Task failed: {e:?}");
                }
            }
            handshaker_tasks.shutdown().await;

            while let Some(result) = tasks.lock().await.try_join_next() {
                if let Err(e) = result {
                    eprintln!("Task failed: {e:?}");
                }
            }
            tasks.lock().await.shutdown().await;
        }
        Downloader::Interrupted => {
            handshaker_tasks.shutdown().await;
            tasks.lock().await.shutdown().await;
        }
    }
}

fn parse_arguments() -> clap::ArgMatches {
    clap::Command::new("simple_torrent")
        .version("1.0")
        .author("Andrew <amiller4421@gmail.com>")
        .about("Simple torrent downloading")
        .arg(
            clap::Arg::new("file")
                .short('f')
                .required(true)
                .value_name("FILE")
                .help("Location of the torrent file"),
        )
        .arg(
            clap::Arg::new("dir")
                .short('d')
                .value_name("DIR")
                .help("Download directory to use"),
        )
        .arg(
            clap::Arg::new("peer")
                .short('p')
                .value_name("PEER")
                .help("Single peer to connect to of the form addr:port"),
        )
        .get_matches()
}

fn extract_arguments(matches: &clap::ArgMatches) -> (String, String, String) {
    let torrent_file_path = matches.get_one::<String>("file").unwrap().to_string();
    let download_directory = matches.get_one::<String>("dir").unwrap().to_string();
    let peer_address = matches.get_one::<String>("peer").unwrap().to_string();
    (torrent_file_path, download_directory, peer_address)
}

fn load_and_parse_torrent_file(torrent_file_path: &str) -> (Metainfo, InfoHash) {
    let mut torrent_file_bytes = Vec::new();
    std::fs::File::open(torrent_file_path)
        .unwrap()
        .read_to_end(&mut torrent_file_bytes)
        .unwrap();

    let metainfo = Metainfo::from_bytes(torrent_file_bytes).unwrap();
    let info_hash = metainfo.info().info_hash();
    (metainfo, info_hash)
}

async fn setup(download_directory: String) -> ((NativeDiskManager, PeerManager, TcpHandshaker), JoinSet<()>) {
    // Setup disk manager for handling file operations
    let disk_manager = setup_disk_manager(&download_directory);

    // Setup peer manager for managing peer communication
    let peer_manager = setup_peer_manager();

    // Setup handshaker for managing peer connections
    let (handshaker, handshaker_tasks) = setup_handshaker().await;

    ((disk_manager, peer_manager, handshaker), handshaker_tasks)
}

async fn downloader(
    tasks: Arc<Mutex<JoinSet<()>>>,
    managers: (NativeDiskManager, PeerManager, TcpHandshaker),
    peer_address: String,
    metainfo: Metainfo,
    info_hash: InfoHash,
) {
    let (disk_manager, peer_manager, handshaker) = managers;

    // Setup disk manager for handling file operations
    let (mut disk_manager_sender, disk_manager_receiver) = disk_manager.into_parts();

    // Setup peer manager for managing peer communication
    let (peer_manager_sender, peer_manager_receiver) = peer_manager.into_parts();

    // Setup handshaker for managing peer connections
    let (mut handshaker_sender, handshaker_receiver) = handshaker.into_parts();

    // Handle new incoming connections
    tasks
        .lock()
        .await
        .spawn(handle_new_connections(handshaker_receiver, peer_manager_sender.clone()));

    // Shared state for managing disk requests
    let disk_request_map = Arc::new(Mutex::new(HashMap::new()));
    let (selection_sender, selection_receiver) = mpsc::channel(50);

    // Handle messages from the peer manager
    tasks.lock().await.spawn(handle_peer_manager_messages(
        peer_manager_receiver,
        info_hash,
        disk_request_map.clone(),
        selection_sender.clone(),
        disk_manager_sender.clone(),
    ));

    // Handle messages from the disk manager
    tasks.lock().await.spawn(handle_disk_manager_messages(
        disk_manager_receiver,
        disk_request_map.clone(),
        selection_sender.clone(),
        peer_manager_sender.clone(),
    ));

    // Generate piece requests for the torrent
    let piece_requests = generate_piece_requests(metainfo.info(), 16 * 1024);

    // Add the torrent to the disk manager
    disk_manager_sender
        .send(IDiskMessage::AddTorrent(metainfo.clone()))
        .await
        .unwrap();

    // Handle existing pieces and update the selection receiver
    let (selection_receiver, piece_requests, current_pieces) =
        handle_existing_pieces(selection_receiver, piece_requests, 0).await;

    // Initiate connection to the specified peer
    handshaker_sender
        .send(InitiateMessage::new(
            Protocol::BitTorrent,
            info_hash,
            peer_address.parse().unwrap(),
        ))
        .await
        .unwrap();

    // Print current status of pieces and requests
    let total_pieces = metainfo.info().pieces().count();
    println!(
        "Current Pieces: {}\nTotal Pieces: {}\nRequests Left: {}",
        current_pieces,
        total_pieces,
        piece_requests.len()
    );

    // Handle selection messages and manage piece requests
    let () = handle_selection_messages(
        selection_receiver,
        peer_manager_sender,
        piece_requests,
        None,
        false,
        0,
        current_pieces,
        total_pieces,
    )
    .await;
}

type NativeDiskManager = DiskManager<FileHandleCache<NativeFileSystem>>;

fn setup_disk_manager(download_directory: &str) -> DiskManager<FileHandleCache<NativeFileSystem>> {
    let filesystem = FileHandleCache::new(NativeFileSystem::with_directory(download_directory), 100);

    DiskManagerBuilder::new()
        .with_sink_buffer_capacity(1)
        .with_stream_buffer_capacity(0)
        .build(Arc::new(filesystem))
}

type TcpHandshaker = Handshaker<TcpStream>;

async fn setup_handshaker() -> (TcpHandshaker, JoinSet<()>) {
    HandshakerBuilder::new()
        .with_peer_id(PeerId::from_hash("-BI0000-000000000000".as_bytes()).unwrap())
        .with_config(HandshakerConfig::default().with_wait_buffer_size(0).with_done_buffer_size(0))
        .build(TcpTransport)
        .await
        .unwrap()
}

type PeerManager = peer::PeerManager<
    Framed<TcpStream, PeerProtocolCodec<PeerWireProtocol<NullProtocol>>>,
    PeerWireProtocolMessage<NullProtocol>,
>;

#[allow(clippy::type_complexity)]
fn setup_peer_manager() -> PeerManager {
    PeerManagerBuilder::new()
        .with_sink_buffer_capacity(0)
        .with_stream_buffer_capacity(0)
        .build()
}

async fn handle_new_connections(
    handshaker_receiver: HandshakerStream<TcpStream>,
    peer_manager_sender: PeerManagerSink<
        Framed<TcpStream, PeerProtocolCodec<PeerWireProtocol<NullProtocol>>>,
        PeerWireProtocolMessage<NullProtocol>,
    >,
) {
    let new_connections = handshaker_receiver
        .filter_map(|item| async move { Some(item.expect("it should not have a failure when making the handshake")) });

    let new_peers = new_connections.map(|message| {
        let (_, _, hash, peer_id, address, socket) = message.into_parts();
        let framed_socket = Decoder::framed(
            PeerProtocolCodec::with_max_payload(PeerWireProtocol::new(NullProtocol::new()), 24 * 1024),
            socket,
        );

        let peer_info = PeerInfo::new(address, peer_id, hash, Extensions::new());

        Ok(Ok(PeerManagerInputMessage::AddPeer(peer_info, framed_socket)))
    });

    new_peers.forward(peer_manager_sender.clone()).await.unwrap();
}

async fn handle_peer_manager_messages(
    mut peer_manager_receiver: PeerManagerStream<
        Framed<TcpStream, PeerProtocolCodec<PeerWireProtocol<NullProtocol>>>,
        PeerWireProtocolMessage<NullProtocol>,
    >,
    info_hash: InfoHash,
    disk_request_map: Arc<Mutex<HashMap<BlockMetadata, Vec<PeerInfo>>>>,
    selection_sender: mpsc::Sender<PeerSelectionState>,
    disk_manager_sender: DiskManagerSink<FileHandleCache<NativeFileSystem>>,
) {
    while let Some(result) = peer_manager_receiver.next().await {
        let opt_message = match result {
            Ok(PeerManagerOutputMessage::ReceivedMessage(peer_info, message)) => match message {
                PeerWireProtocolMessage::Choke => Some(Either::Left(PeerSelectionState::Choke(peer_info))),
                PeerWireProtocolMessage::UnChoke => Some(Either::Left(PeerSelectionState::UnChoke(peer_info))),
                PeerWireProtocolMessage::Interested => Some(Either::Left(PeerSelectionState::Interested(peer_info))),
                PeerWireProtocolMessage::UnInterested => Some(Either::Left(PeerSelectionState::UnInterested(peer_info))),
                PeerWireProtocolMessage::Have(have_message) => {
                    Some(Either::Left(PeerSelectionState::Have(peer_info, have_message)))
                }
                PeerWireProtocolMessage::BitField(bitfield_message) => {
                    Some(Either::Left(PeerSelectionState::BitField(peer_info, bitfield_message)))
                }
                PeerWireProtocolMessage::Request(request_message) => {
                    let block_metadata = BlockMetadata::new(
                        info_hash,
                        u64::from(request_message.piece_index()),
                        u64::from(request_message.block_offset()),
                        request_message.block_length(),
                    );
                    let mut request_map_mut = disk_request_map.lock().await;

                    let block_entry = request_map_mut.entry(block_metadata);
                    let peers_requested = block_entry.or_insert(Vec::new());

                    peers_requested.push(peer_info);

                    Some(Either::Right(IDiskMessage::LoadBlock(BlockMut::new(
                        block_metadata,
                        BytesMut::with_capacity(block_metadata.block_length()),
                    ))))
                }
                PeerWireProtocolMessage::Piece(piece_message) => {
                    let block_metadata = BlockMetadata::new(
                        info_hash,
                        u64::from(piece_message.piece_index()),
                        u64::from(piece_message.block_offset()),
                        piece_message.block_length(),
                    );

                    Some(Either::Right(IDiskMessage::ProcessBlock(Block::new(
                        block_metadata,
                        piece_message.block(),
                    ))))
                }
                _ => None,
            },
            Ok(PeerManagerOutputMessage::PeerAdded(peer_info)) => Some(Either::Left(PeerSelectionState::NewPeer(peer_info))),
            Ok(PeerManagerOutputMessage::PeerRemoved(peer_info)) => {
                println!("Removed Peer {peer_info:?} From The Peer Manager");
                Some(Either::Left(PeerSelectionState::RemovedPeer(peer_info)))
            }
            Ok(PeerManagerOutputMessage::PeerDisconnect(peer_info)) => {
                println!("Peer {peer_info:?} Disconnected From Us");
                Some(Either::Left(PeerSelectionState::RemovedPeer(peer_info)))
            }
            Err(PeerManagerOutputError::PeerError(peer_info, error)) => {
                println!("Peer {peer_info:?} Disconnected With Error: {error:?}");
                Some(Either::Left(PeerSelectionState::RemovedPeer(peer_info)))
            }

            Err(_) | Ok(PeerManagerOutputMessage::SentMessage(_, _)) => None,
        };

        if let Some(message) = opt_message {
            match message {
                Either::Left(selection_message) => {
                    if (selection_sender.clone().send(selection_message).await).is_err() {
                        break;
                    }
                }
                Either::Right(disk_message) => {
                    if (disk_manager_sender.clone().send(disk_message).await).is_err() {
                        break;
                    }
                }
            }
        }
    }
}

async fn handle_disk_manager_messages(
    mut disk_manager_receiver: DiskManagerStream,
    disk_request_map: Arc<Mutex<HashMap<BlockMetadata, Vec<PeerInfo>>>>,
    selection_sender: mpsc::Sender<PeerSelectionState>,
    peer_manager_sender: PeerManagerSink<
        Framed<TcpStream, PeerProtocolCodec<PeerWireProtocol<NullProtocol>>>,
        PeerWireProtocolMessage<NullProtocol>,
    >,
) {
    while let Some(result) = disk_manager_receiver.next().await {
        let opt_message = match result {
            Ok(ODiskMessage::BlockLoaded(block)) => {
                let (metadata, block) = block.into_parts();

                let mut request_map_mut = disk_request_map.lock().await;
                let peer_list = request_map_mut.get_mut(&metadata).unwrap();
                let peer_info = peer_list.remove(0);

                let piece_message = PieceMessage::new(
                    metadata.piece_index().try_into().unwrap(),
                    metadata.block_offset().try_into().unwrap(),
                    block.freeze(),
                );
                let pwp_message = PeerWireProtocolMessage::Piece(piece_message);

                Some(Either::Right(PeerManagerInputMessage::SendMessage(peer_info, 0, pwp_message)))
            }
            Ok(ODiskMessage::TorrentAdded(_)) => Some(Either::Left(PeerSelectionState::TorrentAdded)),
            Ok(ODiskMessage::TorrentSynced(_)) => Some(Either::Left(PeerSelectionState::TorrentSynced)),
            Ok(ODiskMessage::FoundGoodPiece(_, index)) => Some(Either::Left(PeerSelectionState::GoodPiece(index))),
            Ok(ODiskMessage::FoundBadPiece(_, index)) => Some(Either::Left(PeerSelectionState::BadPiece(index))),
            Ok(ODiskMessage::BlockProcessed(_)) => Some(Either::Left(PeerSelectionState::BlockProcessed)),
            _ => None,
        };

        if let Some(message) = opt_message {
            match message {
                Either::Left(selection_message) => {
                    if (selection_sender.clone().send(selection_message).await).is_err() {
                        break;
                    }
                }
                Either::Right(peer_message) => {
                    if (peer_manager_sender.clone().send(Ok(peer_message)).await).is_err() {
                        break;
                    }
                }
            }
        }
    }
}

async fn handle_existing_pieces(
    mut selection_receiver: mpsc::Receiver<PeerSelectionState>,
    mut piece_requests: Vec<RequestMessage>,
    mut current_pieces: usize,
) -> (mpsc::Receiver<PeerSelectionState>, Vec<RequestMessage>, usize) {
    loop {
        match selection_receiver.next().await {
            Some(PeerSelectionState::GoodPiece(index)) => {
                piece_requests.retain(|req| u64::from(req.piece_index()) != index);
                current_pieces += 1;
            }
            None | Some(PeerSelectionState::TorrentAdded) => {
                break (selection_receiver, piece_requests, current_pieces);
            }
            Some(message) => {
                panic!("Unexpected Message Received In Selection mpsc::Receiver: {message:?}");
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_selection_messages(
    mut selection_receiver: mpsc::Receiver<PeerSelectionState>,
    mut peer_manager_sender: PeerManagerSink<
        Framed<TcpStream, PeerProtocolCodec<PeerWireProtocol<NullProtocol>>>,
        PeerWireProtocolMessage<NullProtocol>,
    >,
    mut piece_requests: Vec<RequestMessage>,
    mut optional_peer: Option<PeerInfo>,
    mut is_unchoked: bool,
    mut pending_blocks: usize,
    mut current_pieces: usize,
    total_pieces: usize,
) {
    while let Some(state) = selection_receiver.next().await {
        let control_messages = match state {
            PeerSelectionState::BlockProcessed => {
                pending_blocks -= 1;
                vec![]
            }
            PeerSelectionState::Choke(_) => {
                is_unchoked = false;
                vec![]
            }
            PeerSelectionState::UnChoke(_) => {
                is_unchoked = true;
                vec![]
            }
            PeerSelectionState::NewPeer(peer_info) => {
                optional_peer = Some(peer_info);
                vec![
                    PeerManagerInputMessage::SendMessage(peer_info, 0, PeerWireProtocolMessage::Interested),
                    PeerManagerInputMessage::SendMessage(peer_info, 0, PeerWireProtocolMessage::UnChoke),
                ]
            }
            PeerSelectionState::GoodPiece(piece_index) => {
                current_pieces += 1;

                if let Some(peer_info) = optional_peer {
                    vec![PeerManagerInputMessage::SendMessage(
                        peer_info,
                        0,
                        PeerWireProtocolMessage::Have(HaveMessage::new(piece_index.try_into().unwrap())),
                    )]
                } else {
                    vec![]
                }
            }
            PeerSelectionState::RemovedPeer(peer_info) => {
                eprintln!("Peer {peer_info:?} Got Disconnected");
                vec![]
            }
            PeerSelectionState::BadPiece(_) => {
                eprintln!("Peer Gave Us Bad Piece");
                vec![]
            }
            _ => vec![],
        };

        if current_pieces == total_pieces {
            println!("All pieces have been successfully downloaded.");
            break;
        } else if let Some(peer_info) = optional_peer {
            let next_piece_requests = if is_unchoked {
                let take_blocks = std::cmp::min(MAX_PENDING_BLOCKS - pending_blocks, piece_requests.len());
                pending_blocks += take_blocks;

                piece_requests
                    .drain(0..take_blocks)
                    .map(move |item| {
                        Ok::<_, _>(PeerManagerInputMessage::SendMessage(
                            peer_info,
                            0,
                            PeerWireProtocolMessage::Request(item),
                        ))
                    })
                    .collect()
            } else {
                vec![]
            };

            // First, send any control messages, then, send any more piece requests
            if let Err(e) = peer_manager_sender
                .send_all(&mut stream::iter(
                    control_messages.into_iter().map(Ok::<_, _>).map(Ok::<_, _>),
                ))
                .await
            {
                eprintln!("Error sending control messages: {e:?}");
                break;
            }

            if let Err(e) = peer_manager_sender
                .send_all(&mut stream::iter(next_piece_requests).map(Ok::<_, _>))
                .await
            {
                eprintln!("Error sending piece requests: {e:?}");
                break;
            }
        }
    }
}

/// Generate a mapping of piece index to list of block requests for that piece, given a block size.
///
/// Note, most clients will drop connections for peers requesting block sizes above 16KB.
fn generate_piece_requests(info: &Info, block_size: usize) -> Vec<RequestMessage> {
    let mut requests = Vec::new();

    // Grab our piece length, and the sum of the lengths of each file in the torrent
    let piece_length: u64 = info.piece_length();
    let mut total_file_length: u64 = info.files().map(metainfo::File::length).sum();

    // Loop over each piece (keep subtracting total file length by piece size, use cmp::min to handle last, smaller piece)
    let mut piece_index: u64 = 0;
    while total_file_length != 0 {
        let next_piece_length = std::cmp::min(total_file_length, piece_length);

        // For all whole blocks, push the block index and block_size
        let whole_blocks = next_piece_length / block_size as u64;
        for block_index in 0..whole_blocks {
            let block_offset = block_index * block_size as u64;

            requests.push(RequestMessage::new(
                piece_index.try_into().unwrap(),
                block_offset.try_into().unwrap(),
                block_size,
            ));
        }

        // Check for any last smaller block within the current piece
        let partial_block_length = next_piece_length % block_size as u64;
        if partial_block_length != 0 {
            let block_offset = whole_blocks * block_size as u64;

            requests.push(RequestMessage::new(
                piece_index.try_into().unwrap(),
                block_offset.try_into().unwrap(),
                partial_block_length.try_into().unwrap(),
            ));
        }

        // Take this piece out of the total length, increment to the next piece
        total_file_length -= next_piece_length;
        piece_index += 1;
    }

    requests
}
