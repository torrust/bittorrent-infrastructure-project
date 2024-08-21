use std::sync::mpsc;
use std::thread::{self};
use std::time::{Duration, Instant};

use common::{tracing_stderr_init, MockDispatcher, MockMessage, INIT, LOOPBACK_IPV4};
use tracing::level_filters::LevelFilter;
use umio::ELoopBuilder;

mod common;

#[test]
fn positive_send_notify() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::ERROR);
    });

    let (mut eloop, _eloop_socket, _shutdown_handle) = ELoopBuilder::new().bind_address(LOOPBACK_IPV4).build().unwrap();

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

    let token = 5;
    let timeout_at = Instant::now() + Duration::from_millis(50);
    dispatch_send.send(MockMessage::SendTimeout(token, timeout_at)).unwrap();
    thread::sleep(Duration::from_millis(300));

    let res = dispatch_recv.try_recv();

    dispatch_send.send(MockMessage::Shutdown).unwrap();
    handle.join().unwrap();

    match res {
        Ok(MockMessage::TimeoutReceived(tkn)) => {
            assert_eq!(tkn, token);
        }
        Ok(other) => panic!("Received Other: {other:?}"),
        Err(e) => panic!("Received Error: {e}"),
    }
}
