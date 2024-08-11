use std::io::Write as _;
use std::net::SocketAddr;
use std::sync::{Arc, Once};
use std::time::Duration;

use clap::{Arg, ArgMatches, Command};
use dht::handshaker_trait::HandshakerTrait;
use dht::{DhtBuilder, DhtEvent, Router};
use futures::future::{BoxFuture, Either};
use futures::{FutureExt, Sink, SinkExt as _, StreamExt};
use handshake::transports::TcpTransport;
use handshake::{
    DiscoveryInfo, Extension, Extensions, HandshakerBuilder, HandshakerConfig, InfoHash, InitiateMessage, PeerId, Protocol,
};
use hex::FromHex;
use metainfo::Metainfo;
use peer::messages::builders::ExtendedMessageBuilder;
use peer::messages::{BitsExtensionMessage, PeerExtensionProtocolMessage, PeerWireProtocolMessage};
use peer::protocols::{NullProtocol, PeerExtensionProtocol, PeerWireProtocol};
use peer::{
    PeerInfo, PeerManagerBuilder, PeerManagerInputMessage, PeerManagerOutputError, PeerManagerOutputMessage, PeerProtocolCodec,
};
use select::discovery::{IDiscoveryMessage, ODiscoveryMessage, UtMetadataModule};
use select::{ControlMessage, IExtendedMessage, IUberMessage, OUberMessage, UberModuleBuilder};
use tokio::signal;
use tokio_util::codec::Framed;
use tracing::level_filters::LevelFilter;

pub static INIT: Once = Once::new();

// Legacy Handshaker, when bip_dht is migrated, it will accept S directly
struct LegacyHandshaker<S> {
    port: u16,
    id: PeerId,
    sender: S,
}

impl<S> LegacyHandshaker<S>
where
    S: DiscoveryInfo + Unpin,
{
    pub fn new(sink: S) -> LegacyHandshaker<S> {
        LegacyHandshaker {
            port: sink.port(),
            id: sink.peer_id(),
            sender: sink,
        }
    }
}

impl<S> HandshakerTrait for LegacyHandshaker<S>
where
    S: Sink<InitiateMessage> + Send + Unpin,
    S::Error: std::fmt::Debug,
{
    type MetadataEnvelope = ();

    fn id(&self) -> PeerId {
        self.id
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn connect(&mut self, _expected: Option<PeerId>, hash: InfoHash, addr: SocketAddr) -> BoxFuture<'_, ()> {
        async move {
            self.sender
                .send(InitiateMessage::new(Protocol::BitTorrent, hash, addr))
                .await
                .unwrap();
        }
        .boxed()
    }

    fn metadata(&mut self, _data: ()) {}
}

fn parse_arguments() -> ArgMatches {
    Command::new("get_metadata")
        .version("1.0")
        .author("Andrew <amiller4421@gmail.com>")
        .about("Download torrent file from info hash")
        .arg(
            Arg::new("infohash")
                .short('i')
                .long("infohash")
                .required(true)
                .value_name("INFOHASH")
                .help("InfoHash of the torrent"),
        )
        .arg(
            Arg::new("output")
                .short('f')
                .long("output")
                .required(true)
                .value_name("OUTPUT")
                .help("Output to write the torrent file to"),
        )
        .get_matches()
}

fn extract_arguments(matches: &ArgMatches) -> (String, String) {
    let hash = matches.get_one::<String>("infohash").unwrap().to_string();
    let output = matches.get_one::<String>("output").unwrap().to_string();
    (hash, output)
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

enum SendUber {
    Finished(Result<(), select::error::Error>),
    Interrupted,
}

enum MainDht {
    Finished(Box<Metainfo>),
    Interrupted,
}

#[allow(clippy::too_many_lines)]
#[tokio::main]
async fn main() {
    INIT.call_once(|| {
        tracing_stdout_init(LevelFilter::TRACE);
    });

    // Parse command-line arguments
    let matches = parse_arguments();
    let (hash, output) = extract_arguments(&matches);

    let hash: Vec<u8> = FromHex::from_hex(hash).expect("Invalid hex in hash argument");
    let info_hash = InfoHash::from_hash(&hash[..]).expect("Failed to create InfoHash");

    // Activate the extension protocol via the handshake bits
    let mut extensions = Extensions::new();
    extensions.add(Extension::ExtensionProtocol);

    // Create a handshaker that can initiate connections with peers
    let (handshaker, mut tasks) = HandshakerBuilder::new()
        .with_extensions(extensions)
        .with_config(
            // Set a low handshake timeout so we don't wait on peers that aren't listening on tcp
            HandshakerConfig::default().with_connect_timeout(Duration::from_millis(500)),
        )
        .build(TcpTransport)
        .await
        .expect("it should build a handshaker pair");
    let (handshaker_send, mut handshaker_recv) = handshaker.into_parts();

    // Create a peer manager that will hold our peers and heartbeat/send messages to them
    let (mut peer_manager_send, peer_manager_recv) = PeerManagerBuilder::new().build().into_parts();

    // Hook up a future that feeds incoming (handshaken) peers over to the peer manager
    tasks.spawn(async move {
        while let Some(complete_msg) = handshaker_recv.next().await {
            let (_, extensions, hash, pid, addr, sock) = complete_msg.unwrap().into_parts();

            if extensions.contains(Extension::ExtensionProtocol) {
                let peer = Framed::new(
                    sock,
                    PeerProtocolCodec::with_max_payload(
                        PeerWireProtocol::new(PeerExtensionProtocol::new(NullProtocol::new())),
                        24 * 1024,
                    ),
                );

                let peer_info = PeerInfo::new(addr, pid, hash, extensions);

                peer_manager_send
                    .send(Ok(PeerManagerInputMessage::AddPeer(peer_info, peer)))
                    .await
                    .unwrap();
            } else {
                panic!("Chosen Peer Does Not Support Extended Messages");
            }
        }
    });

    // Create our UtMetadata selection module
    let (mut uber_send, mut uber_recv) = {
        let builder = UberModuleBuilder::new().with_extended_builder(Some(ExtendedMessageBuilder::new()));
        let module = UtMetadataModule::new();
        builder.discovery.lock().unwrap().push(Arc::new(module));

        builder
    }
    .build()
    .into_parts();

    // Tell the uber module we want to download metainfo for the given hash
    let send_to_uber = uber_send
        .send(IUberMessage::Discovery(Box::new(IDiscoveryMessage::DownloadMetainfo(
            info_hash,
        ))))
        .boxed();

    // Await either the sending to uber or the Ctrl-C signal
    let send_to_uber = tokio::select! {
        res = send_to_uber => SendUber::Finished(res),
        () = ctrl_c() => SendUber::Interrupted,
    };

    let () = match send_to_uber {
        SendUber::Finished(Ok(())) => (),

        SendUber::Finished(Err(e)) => {
            tracing::warn!("send to uber failed with error: {e}");
            tasks.shutdown().await;
            return;
        }

        SendUber::Interrupted => {
            tracing::warn!("setup was canceled...");
            tasks.shutdown().await;
            return;
        }
    };

    let timer = futures::stream::unfold(tokio::time::interval(Duration::from_millis(100)), |mut interval| async move {
        interval.tick().await;
        Some(((), interval))
    });

    let mut merged_recv = futures::stream::select(peer_manager_recv.map(Either::Left), timer.map(Either::Right)).boxed();

    // Hook up a future that receives messages from the peer manager
    tasks.spawn(async move {
        let mut uber_send = uber_send.clone();

        while let Some(item) = merged_recv.next().await {
            let message = if let Either::Left(message) = item {
                match message {
                    Ok(PeerManagerOutputMessage::PeerAdded(info)) => {
                        tracing::info!("Connected To Peer: {info:?}");
                        IUberMessage::Control(Box::new(ControlMessage::PeerConnected(info)))
                    }
                    Ok(PeerManagerOutputMessage::PeerRemoved(info)) => {
                        tracing::info!("We Removed Peer {info:?} From The Peer Manager");
                        IUberMessage::Control(Box::new(ControlMessage::PeerDisconnected(info)))
                    }
                    Ok(PeerManagerOutputMessage::SentMessage(_, _)) => todo!(),
                    Ok(PeerManagerOutputMessage::ReceivedMessage(info, message)) => match message {
                        PeerWireProtocolMessage::BitsExtension(message) => match message {
                            BitsExtensionMessage::Extended(extended) => {
                                IUberMessage::Extended(Box::new(IExtendedMessage::ReceivedExtendedMessage(info, extended)))
                            }
                            BitsExtensionMessage::Port(_) => unimplemented!(),
                        },
                        PeerWireProtocolMessage::ProtExtension(message) => match message {
                            Ok(PeerExtensionProtocolMessage::UtMetadata(message)) => {
                                IUberMessage::Discovery(Box::new(IDiscoveryMessage::ReceivedUtMetadataMessage(info, message)))
                            }
                            _ => unimplemented!(),
                        },
                        _ => unimplemented!(),
                    },
                    Ok(PeerManagerOutputMessage::PeerDisconnect(info)) => {
                        tracing::info!("Peer {info:?} Disconnected From Us");
                        IUberMessage::Control(Box::new(ControlMessage::PeerDisconnected(info)))
                    }
                    Err(e) => {
                        let info = match e {
                            PeerManagerOutputError::PeerError(info, _)
                            | PeerManagerOutputError::PeerErrorAndMissing(info, _)
                            | PeerManagerOutputError::PeerRemovedAndMissing(info)
                            | PeerManagerOutputError::PeerDisconnectedAndMissing(info) => info,
                        };

                        tracing::info!("Peer {info:?} Disconnected With Error: {e:?}");
                        IUberMessage::Control(Box::new(ControlMessage::PeerDisconnected(info)))
                    }
                }
            } else {
                IUberMessage::Control(Box::new(ControlMessage::Tick(Duration::from_millis(100))))
            };

            uber_send.send(message).await.unwrap();
        }
    });

    // Setup the dht which will be the only peer discovery service we use in this example
    let legacy_handshaker = LegacyHandshaker::new(handshaker_send);

    let main_dht = async move {
        let dht = DhtBuilder::with_router(Router::uTorrent)
            .set_read_only(false)
            .start_mainline(legacy_handshaker)
            .await
            .expect("it should start the dht mainline");

        tracing::info!("Bootstrapping Dht...");
        while let Some(message) = dht.events().await.next().await {
            if let DhtEvent::BootstrapCompleted = message {
                break;
            }
        }
        tracing::info!("Bootstrap Complete...");

        dht.search(info_hash, true).await;

        loop {
            if let Some(Ok(OUberMessage::Discovery(ODiscoveryMessage::DownloadedMetainfo(metainfo)))) = uber_recv.next().await {
                break metainfo;
            }
        }
    }
    .boxed();

    // Await either the sending to uber or the Ctrl-C signal
    let main_dht = tokio::select! {
        res = main_dht => MainDht::Finished(Box::new(res)),
        () = ctrl_c() => MainDht::Interrupted,
    };

    let metainfo = match main_dht {
        MainDht::Finished(metainfo) => metainfo,

        MainDht::Interrupted => {
            tracing::warn!("setup was canceled...");
            tasks.shutdown().await;
            return;
        }
    };

    // Write the metainfo file out to the user provided path
    std::fs::File::create(output)
        .expect("Failed to create output file")
        .write_all(&metainfo.to_bytes())
        .expect("Failed to write metainfo to file");

    tasks.shutdown().await;
}
