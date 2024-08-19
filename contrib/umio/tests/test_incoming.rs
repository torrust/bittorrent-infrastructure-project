use std::net::UdpSocket;
use std::thread::{self};
use std::time::Duration;

use common::{MockDispatcher, MockMessage};
use umio::ELoopBuilder;

mod common;

#[test]
fn positive_receive_incoming_message() {
    let eloop_addr = "127.0.0.1:5050".parse().unwrap();
    let mut eloop = ELoopBuilder::new().bind_address(eloop_addr).build().unwrap();

    let (dispatcher, dispatch_recv) = MockDispatcher::new();
    let dispatch_send = eloop.channel();

    thread::spawn(move || {
        eloop.run(dispatcher).unwrap();
    });
    thread::sleep(Duration::from_millis(50));

    let socket_addr = "127.0.0.1:5051".parse().unwrap();
    let socket = UdpSocket::bind(socket_addr).unwrap();
    let message = b"This Is A Test Message";

    socket.send_to(&message[..], eloop_addr).unwrap();
    thread::sleep(Duration::from_millis(50));

    match dispatch_recv.try_recv() {
        Ok(MockMessage::MessageReceived(msg, addr)) => {
            assert_eq!(&msg[..], &message[..]);

            assert_eq!(addr, socket_addr);
        }
        _ => panic!("ELoop Failed To Receive Incoming Message"),
    }

    dispatch_send.send(MockMessage::Shutdown).unwrap();
}
