use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use futures::channel::mpsc;
use futures::{Sink, SinkExt as _, Stream, StreamExt as _};

#[derive(Debug)]
pub struct ConnectedChannel<I, O> {
    send: mpsc::Sender<I>,
    recv: Arc<Mutex<mpsc::Receiver<O>>>,
}

impl<I, O> Clone for ConnectedChannel<I, O> {
    fn clone(&self) -> Self {
        Self {
            send: self.send.clone(),
            recv: self.recv.clone(),
        }
    }
}

impl<I, O> Sink<I> for ConnectedChannel<I, O> {
    type Error = std::io::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.send
            .poll_ready_unpin(cx)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::ConnectionAborted, e))
    }

    fn start_send(mut self: Pin<&mut Self>, item: I) -> Result<(), Self::Error> {
        self.send
            .start_send_unpin(item)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::ConnectionAborted, e))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.send
            .poll_flush_unpin(cx)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::ConnectionAborted, e))
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.send
            .poll_close_unpin(cx)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::ConnectionAborted, e))
    }
}

impl<I, O> Stream for ConnectedChannel<I, O> {
    type Item = O;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let Ok(recv) = self.recv.try_lock() else {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        };

        Pin::new(recv).poll_next_unpin(cx)
    }
}

#[must_use]
pub fn connected_channel<I, O>(capacity: usize) -> (ConnectedChannel<I, O>, ConnectedChannel<O, I>) {
    let (send_one, recv_one) = futures::channel::mpsc::channel(capacity);
    let (send_two, recv_two) = futures::channel::mpsc::channel(capacity);

    (
        ConnectedChannel {
            send: send_one,
            recv: Arc::new(Mutex::new(recv_two)),
        },
        ConnectedChannel {
            send: send_two,
            recv: Arc::new(Mutex::new(recv_one)),
        },
    )
}
