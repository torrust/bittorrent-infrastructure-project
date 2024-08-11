//! `Stream` portion of the `Handshaker` for completed handshakes.

use futures::sync::mpsc::Receiver;
use futures::{Poll, Stream};

use crate::CompleteMessage;

#[allow(clippy::module_name_repetitions)]
pub struct HandshakerStream<S> {
    recv: Receiver<CompleteMessage<S>>,
}

impl<S> HandshakerStream<S> {
    pub(super) fn new(recv: Receiver<CompleteMessage<S>>) -> HandshakerStream<S> {
        HandshakerStream { recv }
    }
}

impl<S> Stream for HandshakerStream<S> {
    type Item = CompleteMessage<S>;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<CompleteMessage<S>>, ()> {
        self.recv.poll()
    }
}
