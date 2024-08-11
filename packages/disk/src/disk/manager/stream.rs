//! `DiskManagerStream` which is the stream portion of a `DiskManager`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crossbeam::queue::SegQueue;
use futures::sync::mpsc::Receiver;
use futures::task::Task;
use futures::{Async, Poll, Stream};
use log::info;

use crate::ODiskMessage;

#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct DiskManagerStream {
    recv: Receiver<ODiskMessage>,
    cur_capacity: Arc<AtomicUsize>,
    task_queue: Arc<SegQueue<Task>>,
}

impl DiskManagerStream {
    pub(super) fn new(
        recv: Receiver<ODiskMessage>,
        cur_capacity: Arc<AtomicUsize>,
        task_queue: Arc<SegQueue<Task>>,
    ) -> DiskManagerStream {
        DiskManagerStream {
            recv,
            cur_capacity,
            task_queue,
        }
    }

    fn complete_work(&self) {
        self.cur_capacity.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Stream for DiskManagerStream {
    type Item = ODiskMessage;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<ODiskMessage>, ()> {
        info!("Polling DiskManagerStream For ODiskMessage");

        match self.recv.poll() {
            res @ Ok(Async::Ready(Some(
                ODiskMessage::TorrentAdded(_)
                | ODiskMessage::TorrentRemoved(_)
                | ODiskMessage::TorrentSynced(_)
                | ODiskMessage::BlockLoaded(_)
                | ODiskMessage::BlockProcessed(_),
            ))) => {
                self.complete_work();

                info!("Notifying DiskManager That We Can Submit More Work");

                while let Some(task) = self.task_queue.pop() {
                    task.notify();
                }

                res
            }
            other => other,
        }
    }
}
