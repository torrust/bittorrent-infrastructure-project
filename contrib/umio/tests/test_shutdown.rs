use std::sync::mpsc;

use common::{tracing_stderr_init, MockDispatcher, MockMessage, INIT, LOOPBACK_IPV4};
use tracing::level_filters::LevelFilter;
use umio::ELoopBuilder;

mod common;

#[test]
fn positive_execute_shutdown() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::ERROR);
    });

    let (mut eloop, _eloop_socket, _shutdown_handle) = ELoopBuilder::new().bind_address(LOOPBACK_IPV4).build().unwrap();

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

    dispatch_send.send(MockMessage::Shutdown).unwrap();
    handle.join().unwrap();

    assert!(dispatch_send.send(MockMessage::SendNotify).is_err());
}
