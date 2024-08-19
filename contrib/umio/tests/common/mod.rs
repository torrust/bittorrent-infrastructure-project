use std::net::SocketAddr;
use std::sync::mpsc::{self};

use umio::{Dispatcher, Provider};

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
    SendTimeout(u32, u64),

    Shutdown,
}

impl MockDispatcher {
    pub fn new() -> (MockDispatcher, mpsc::Receiver<MockMessage>) {
        let (send, recv) = mpsc::channel();

        (MockDispatcher { send }, recv)
    }
}

impl Dispatcher for MockDispatcher {
    type Timeout = u32;
    type Message = MockMessage;

    fn incoming(&mut self, _: Provider<'_, Self>, message: &[u8], addr: SocketAddr) {
        let owned_message = message.to_vec();

        self.send.send(MockMessage::MessageReceived(owned_message, addr)).unwrap();
    }

    fn notify(&mut self, mut provider: Provider<'_, Self>, msg: Self::Message) {
        match msg {
            MockMessage::SendMessage(message, addr) => {
                provider.outgoing(|buffer| {
                    for (src, dst) in message.iter().zip(buffer.as_mut().iter_mut()) {
                        *dst = *src;
                    }

                    Some((message.len(), addr))
                });
            }
            MockMessage::SendTimeout(token, delay) => {
                provider.set_timeout(token, delay).unwrap();
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

    fn timeout(&mut self, _: Provider<'_, Self>, token: Self::Timeout) {
        self.send.send(MockMessage::TimeoutReceived(token)).unwrap();
    }
}
