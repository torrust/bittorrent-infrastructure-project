//! `DiskManagerStream` which is the stream portion of a `DiskManager`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use crossbeam::queue::SegQueue;
use futures::channel::mpsc;
use futures::{Stream, StreamExt as _};

use crate::ODiskMessage;

#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct DiskManagerStream {
    recv: mpsc::Receiver<ODiskMessage>,
    pub cur_capacity: Arc<AtomicUsize>,
    wake_queue: Arc<SegQueue<Waker>>,
}

impl DiskManagerStream {
    pub(super) fn new(
        recv: mpsc::Receiver<ODiskMessage>,
        cur_capacity: Arc<AtomicUsize>,
        wake_queue: Arc<SegQueue<Waker>>,
    ) -> DiskManagerStream {
        DiskManagerStream {
            recv,
            cur_capacity,
            wake_queue,
        }
    }

    fn complete_work(&self) -> usize {
        let cap = self.cur_capacity.fetch_sub(1, Ordering::SeqCst) - 1;

        tracing::debug!(
            "Notify next waker: {} that there is space again: {cap}",
            self.wake_queue.len()
        );
        if let Some(waker) = self.wake_queue.pop() {
            waker.wake();
        };

        cap
    }
}

impl Stream for DiskManagerStream {
    type Item = Result<ODiskMessage, ()>;

    fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        tracing::trace!("Polling DiskManagerStream For ODiskMessage");

        match self.recv.poll_next_unpin(cx) {
            Poll::Ready(Some(msg)) => {
                match msg {
                    ODiskMessage::TorrentAdded(_)
                    | ODiskMessage::TorrentRemoved(_)
                    | ODiskMessage::TorrentSynced(_)
                    | ODiskMessage::BlockLoaded(_)
                    | ODiskMessage::BlockProcessed(_) => {
                        self.complete_work();
                    }
                    _ => {}
                }
                Poll::Ready(Some(Ok(msg)))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
