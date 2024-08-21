use std::net::UdpSocket;
use std::sync::mpsc;
use std::time::Duration;

use common::{tracing_stderr_init, MockDispatcher, MockMessage, INIT, LOOPBACK_IPV4};
use tracing::level_filters::LevelFilter;
use umio::ELoopBuilder;

mod common;

/// Tests that an incoming message is correctly received and processed.
#[test]
fn positive_receive_incoming_message() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::ERROR);
    });

    tracing::trace!("Starting test: positive_receive_incoming_message");

    let (mut eloop, eloop_socket, _shutdown_handle) = ELoopBuilder::new().bind_address(LOOPBACK_IPV4).build().unwrap();

    let (dispatcher, dispatch_recv) = MockDispatcher::new();
    let dispatch_send = eloop.channel();

    let handle = {
        let (started_eloop_sender, started_eloop_receiver) = mpsc::sync_channel(0);

        let handle = std::thread::spawn(move || {
            eloop.run(dispatcher, started_eloop_sender).unwrap();
        });

        let () = started_eloop_receiver.recv().unwrap().unwrap();

        handle
    };

    let socket = UdpSocket::bind(LOOPBACK_IPV4).unwrap();
    let socket_addr = socket.local_addr().unwrap();
    let message = b"This Is A Test Message";

    tracing::trace!("Sending message to event loop");
    socket.send_to(&message[..], eloop_socket).unwrap();
    std::thread::sleep(Duration::from_millis(50));

    tracing::trace!("Checking for received message");
    let res: Result<MockMessage, _> = dispatch_recv.try_recv();

    dispatch_send.send(MockMessage::Shutdown).unwrap();
    handle.join().unwrap();

    match res {
        Ok(MockMessage::MessageReceived(msg, addr)) => {
            assert_eq!(&msg[..], &message[..]);
            assert_eq!(addr, socket_addr);
        }
        _ => panic!("ELoop Failed To Receive Incoming Message"),
    };
}
