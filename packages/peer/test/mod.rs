extern crate futures;
extern crate handshake;
extern crate peer;
extern crate tokio_core;
extern crate tokio_io;
extern crate util;

use std::io;

use futures::sink::Sink;
use futures::stream::Stream;
use futures::sync::mpsc::{self, Receiver, Sender};
use futures::{Poll, StartSend};

mod peer_manager_send_backpressure;

pub struct ConnectedChannel<I, O> {
    send: Sender<I>,
    recv: Receiver<O>,
}

impl<I, O> Sink for ConnectedChannel<I, O> {
    type SinkItem = I;
    type SinkError = io::Error;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        self.send
            .start_send(item)
            .map_err(|_| io::Error::new(io::ErrorKind::ConnectionAborted, "Sender Failed To Send"))
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.send
            .poll_complete()
            .map_err(|_| io::Error::new(io::ErrorKind::ConnectionAborted, "Sender Failed To Send"))
    }
}

impl<I, O> Stream for ConnectedChannel<I, O> {
    type Item = O;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.recv
            .poll()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Receiver Failed To Receive"))
    }
}

#[must_use]
pub fn connected_channel<I, O>(capacity: usize) -> (ConnectedChannel<I, O>, ConnectedChannel<O, I>) {
    let (send_one, recv_one) = mpsc::channel(capacity);
    let (send_two, recv_two) = mpsc::channel(capacity);

    (
        ConnectedChannel {
            send: send_one,
            recv: recv_two,
        },
        ConnectedChannel {
            send: send_two,
            recv: recv_one,
        },
    )
}