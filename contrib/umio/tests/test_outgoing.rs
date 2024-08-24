use std::net::UdpSocket;
use std::sync::mpsc;
use std::time::Duration;

use common::{tracing_stderr_init, MockDispatcher, MockMessage, INIT, LOOPBACK_IPV4};
use tracing::level_filters::LevelFilter;
use umio::ELoopBuilder;

mod common;

#[test]
fn positive_send_outgoing_message() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::ERROR);
    });

    tracing::trace!("Starting test: positive_send_outgoing_message");
    let (mut eloop, eloop_socket, _shutdown_handle) = ELoopBuilder::new().bind_address(LOOPBACK_IPV4).build().unwrap();

    let (dispatcher, _) = MockDispatcher::new();
    let dispatch_send = eloop.channel();

    let handle = {
        let (started_eloop_sender, started_eloop_receiver) = mpsc::sync_channel(0);

        let handle = std::thread::spawn(move || {
            eloop.run(dispatcher, started_eloop_sender).unwrap();
        });

        let () = started_eloop_receiver.recv().unwrap().unwrap();

        handle
    };

    let message = b"This Is A Test Message";
    let mut message_recv = [0u8; 22];
    let socket = UdpSocket::bind(LOOPBACK_IPV4).unwrap();
    let socket_addr = socket.local_addr().unwrap(); // Get the actual address

    tracing::trace!("sending message to: {socket_addr}");
    dispatch_send
        .send(MockMessage::SendMessage(message.to_vec(), socket_addr))
        .unwrap();

    tracing::trace!("receiving  message from: {eloop_socket}");
    socket.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let (bytes, addr) = socket.recv_from(&mut message_recv).unwrap();

    dispatch_send.send(MockMessage::Shutdown).unwrap();
    handle.join().unwrap(); // Wait for the event loop to finish

    assert_eq!(bytes, message.len());
    assert_eq!(&message[..], &message_recv[..]);
    assert_eq!(addr, eloop_socket);
}
