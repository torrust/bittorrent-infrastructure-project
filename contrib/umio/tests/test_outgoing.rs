use std::net::UdpSocket;
use std::thread::{self};
use std::time::Duration;

use common::{MockDispatcher, MockMessage};
use umio::ELoopBuilder;

mod common;

#[test]
fn positive_send_outgoing_message() {
    let eloop_addr = "127.0.0.1:5052".parse().unwrap();
    let mut eloop = ELoopBuilder::new().bind_address(eloop_addr).build().unwrap();

    let (dispatcher, _) = MockDispatcher::new();
    let dispatch_send = eloop.channel();

    thread::spawn(move || {
        eloop.run(dispatcher).unwrap();
    });
    thread::sleep(Duration::from_millis(50));

    let message = b"This Is A Test Message";
    let mut message_recv = [0u8; 22];
    let socket_addr = "127.0.0.1:5053".parse().unwrap();
    let socket = UdpSocket::bind(socket_addr).unwrap();
    dispatch_send
        .send(MockMessage::SendMessage(message.to_vec(), socket_addr))
        .unwrap();
    thread::sleep(Duration::from_millis(50));

    let (bytes, addr) = socket.recv_from(&mut message_recv).unwrap();

    assert_eq!(bytes, message.len());
    assert_eq!(&message[..], &message_recv[..]);
    assert_eq!(addr, eloop_addr);

    dispatch_send.send(MockMessage::Shutdown).unwrap();
}
