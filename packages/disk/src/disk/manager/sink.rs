//! `DiskManagerSink` which is the sink portion of a `DiskManager`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crossbeam::queue::SegQueue;
use futures::task::{self, Task};
use futures::{Async, AsyncSink, Poll, Sink, StartSend};
use futures_cpupool::CpuPool;
use log::info;

use crate::disk::tasks;
use crate::disk::tasks::context::DiskManagerContext;
use crate::{FileSystem, IDiskMessage};

#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct DiskManagerSink<F> {
    pool: CpuPool,
    context: DiskManagerContext<F>,
    max_capacity: usize,
    cur_capacity: Arc<AtomicUsize>,
    task_queue: Arc<SegQueue<Task>>,
}

impl<F> Clone for DiskManagerSink<F> {
    fn clone(&self) -> DiskManagerSink<F> {
        DiskManagerSink {
            pool: self.pool.clone(),
            context: self.context.clone(),
            max_capacity: self.max_capacity,
            cur_capacity: self.cur_capacity.clone(),
            task_queue: self.task_queue.clone(),
        }
    }
}

impl<F> DiskManagerSink<F> {
    pub(super) fn new(
        pool: CpuPool,
        context: DiskManagerContext<F>,
        max_capacity: usize,
        cur_capacity: Arc<AtomicUsize>,
        task_queue: Arc<SegQueue<Task>>,
    ) -> DiskManagerSink<F> {
        DiskManagerSink {
            pool,
            context,
            max_capacity,
            cur_capacity,
            task_queue,
        }
    }

    fn try_submit_work(&self) -> bool {
        let cur_capacity = self.cur_capacity.fetch_add(1, Ordering::SeqCst);

        if cur_capacity < self.max_capacity {
            true
        } else {
            self.cur_capacity.fetch_sub(1, Ordering::SeqCst);

            false
        }
    }
}

impl<F> Sink for DiskManagerSink<F>
where
    F: FileSystem + Send + Sync + 'static,
{
    type SinkItem = IDiskMessage;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        info!("Starting Send For DiskManagerSink With IDiskMessage");

        if self.try_submit_work() {
            info!("DiskManagerSink Submitted Work On First Attempt");
            tasks::execute_on_pool(item, &self.pool, self.context.clone());

            return Ok(AsyncSink::Ready);
        }

        // We split the sink and stream, which means these could be polled in different event loops (I think),
        // so we need to add our task, but then try to submit work again, in case the receiver processed work
        // right after we tried to submit the first time.
        info!("DiskManagerSink Failed To Submit Work On First Attempt, Adding Task To Queue");
        self.task_queue.push(task::current());

        if self.try_submit_work() {
            // Receiver will look at the queue but wake us up, even though we don't need it to now...
            info!("DiskManagerSink Submitted Work On Second Attempt");
            tasks::execute_on_pool(item, &self.pool, self.context.clone());

            Ok(AsyncSink::Ready)
        } else {
            // Receiver will look at the queue eventually...
            Ok(AsyncSink::NotReady(item))
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        Ok(Async::Ready(()))
    }
}
