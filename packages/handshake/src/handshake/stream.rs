use std::task::{Context, Poll};

use futures::channel::mpsc;
use futures::{Stream, StreamExt as _};

use crate::CompleteMessage;

/// `Stream` portion of the `Handshaker` for completed handshakes.
#[allow(clippy::module_name_repetitions)]
pub struct HandshakerStream<S> {
    recv: mpsc::Receiver<std::io::Result<CompleteMessage<S>>>,
}

impl<S> HandshakerStream<S> {
    pub(super) fn new(recv: mpsc::Receiver<std::io::Result<CompleteMessage<S>>>) -> HandshakerStream<S> {
        HandshakerStream { recv }
    }
}

impl<S> Stream for HandshakerStream<S> {
    type Item = std::io::Result<CompleteMessage<S>>;

    fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.recv.poll_next_unpin(cx)
    }
}
