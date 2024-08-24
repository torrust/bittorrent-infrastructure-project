use std::net::UdpSocket;
use std::time::Duration;

use common::{tracing_stderr_init, MockTrackerHandler, INIT, LOOPBACK_IPV4};
use tracing::level_filters::LevelFilter;
use utracker::request::{self, RequestType, TrackerRequest};
use utracker::TrackerServer;

mod common;

#[test]
#[allow(unused)]
fn positive_server_dropped() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::ERROR);
    });

    let mock_handler = MockTrackerHandler::new();

    let old_server_socket = {
        let server = TrackerServer::run(LOOPBACK_IPV4, mock_handler).unwrap();
        server.local_addr()
    };
    // Server is now shut down

    let mut send_message = Vec::new();

    let request = TrackerRequest::new(request::CONNECT_ID_PROTOCOL_ID, 0, RequestType::Connect);
    request.write_bytes(&mut send_message).unwrap();

    let socket = UdpSocket::bind(LOOPBACK_IPV4).unwrap();
    socket.send_to(&send_message, old_server_socket);

    let mut receive_message = vec![0u8; 1500];
    socket.set_read_timeout(Some(Duration::from_millis(200)));
    let recv_result = socket.recv_from(&mut receive_message);

    assert!(recv_result.is_err());
}
