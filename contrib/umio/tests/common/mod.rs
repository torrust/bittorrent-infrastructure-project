use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{mpsc, Once};
use std::time::Instant;

use tracing::level_filters::LevelFilter;
use tracing::{instrument, Level};
use umio::{Dispatcher, Provider};

pub const LOOPBACK_IPV4: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0));

#[allow(dead_code)]
pub static INIT: Once = Once::new();

#[allow(dead_code)]
pub fn tracing_stderr_init(filter: LevelFilter) {
    let builder = tracing_subscriber::fmt()
        .with_max_level(filter)
        .with_ansi(true)
        .with_writer(std::io::stderr);

    builder.pretty().with_file(true).init();

    tracing::info!("Logging initialized");
}

#[derive(Debug)]
pub struct MockDispatcher {
    send: mpsc::Sender<MockMessage>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum MockMessage {
    MessageReceived(Vec<u8>, SocketAddr),
    TimeoutReceived(u32),
    NotifyReceived,

    SendNotify,
    SendMessage(Vec<u8>, SocketAddr),
    SendTimeout(u32, Instant),

    Shutdown,
}

impl MockDispatcher {
    #[instrument(skip(), ret(level = Level::TRACE))]
    pub fn new() -> (MockDispatcher, mpsc::Receiver<MockMessage>) {
        let (send, recv) = mpsc::channel();

        (MockDispatcher { send }, recv)
    }
}

impl Dispatcher for MockDispatcher {
    type TimeoutToken = u32;
    type Message = MockMessage;

    #[instrument(skip())]
    fn incoming(&mut self, _: Provider<'_, Self>, message: &[u8], addr: SocketAddr) {
        let owned_message = message.to_vec();
        tracing::trace!("MockDispatcher: Received message from {addr}");
        self.send.send(MockMessage::MessageReceived(owned_message, addr)).unwrap();
    }

    #[instrument(skip(provider))]
    fn notify(&mut self, mut provider: Provider<'_, Self>, msg: Self::Message) {
        tracing::trace!("MockDispatcher: Received notification {msg:?}");
        match msg {
            MockMessage::SendMessage(message, addr) => {
                let _ = provider.set_dest(addr);

                let _ = provider.write(&message).unwrap();

                let () = provider.flush().unwrap();
            }
            MockMessage::SendTimeout(token, when) => {
                provider.set_timeout(token, when).unwrap();
            }
            MockMessage::SendNotify => {
                self.send.send(MockMessage::NotifyReceived).unwrap();
            }
            MockMessage::Shutdown => {
                provider.shutdown();
            }
            _ => panic!("Invalid Message To Send To Dispatcher: {msg:?}"),
        }
    }

    #[instrument(skip())]
    fn timeout(&mut self, _: Provider<'_, Self>, token: Self::TimeoutToken) {
        tracing::trace!("MockDispatcher: Timeout received for token {token}");
        self.send.send(MockMessage::TimeoutReceived(token)).unwrap();
    }
}
