use std::thread::{self};
use std::time::Duration;

use common::{MockDispatcher, MockMessage};
use umio::ELoopBuilder;

mod common;

#[test]
fn positive_send_notify() {
    let eloop_addr = "127.0.0.1:0".parse().unwrap();
    let mut eloop = ELoopBuilder::new().bind_address(eloop_addr).build().unwrap();

    let (dispatcher, dispatch_recv) = MockDispatcher::new();
    let dispatch_send = eloop.channel();

    thread::spawn(move || {
        eloop.run(dispatcher).unwrap();
    });
    thread::sleep(Duration::from_millis(50));

    let token = 5;
    dispatch_send.send(MockMessage::SendTimeout(token, 50)).unwrap();
    thread::sleep(Duration::from_millis(300));

    match dispatch_recv.try_recv() {
        Ok(MockMessage::TimeoutReceived(tkn)) => {
            assert_eq!(tkn, token);
        }
        _ => panic!("ELoop Failed To Receive Timeout"),
    }

    dispatch_send.send(MockMessage::Shutdown).unwrap();
}
