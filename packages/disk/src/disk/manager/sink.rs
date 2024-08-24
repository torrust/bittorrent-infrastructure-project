//! `DiskManagerSink` which is the sink portion of a `DiskManager`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use crossbeam::queue::SegQueue;
use tokio::task::JoinSet;

use crate::disk::tasks;
use crate::disk::tasks::context::DiskManagerContext;
use crate::{FileSystem, IDiskMessage};

#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct DiskManagerSink<F>
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    context: DiskManagerContext<F>,
    max_capacity: usize,
    cur_capacity: Arc<AtomicUsize>,
    wake_queue: Arc<SegQueue<Waker>>,
    task_set: Arc<Mutex<JoinSet<()>>>,
}

impl<F> Clone for DiskManagerSink<F>
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    fn clone(&self) -> DiskManagerSink<F> {
        DiskManagerSink {
            context: self.context.clone(),
            max_capacity: self.max_capacity,
            cur_capacity: self.cur_capacity.clone(),
            wake_queue: self.wake_queue.clone(),
            task_set: self.task_set.clone(),
        }
    }
}

impl<F> DiskManagerSink<F>
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    pub(super) fn new(
        context: DiskManagerContext<F>,
        max_capacity: usize,
        cur_capacity: Arc<AtomicUsize>,
        wake_queue: Arc<SegQueue<Waker>>,
    ) -> DiskManagerSink<F> {
        DiskManagerSink {
            context,
            max_capacity,
            cur_capacity,
            wake_queue,
            task_set: Arc::default(),
        }
    }

    fn try_submit_work(&self, waker: &Waker) -> Result<usize, usize> {
        let cap = self.cur_capacity.fetch_add(1, Ordering::SeqCst) + 1;
        let max = self.max_capacity;

        #[allow(clippy::comparison_chain)]
        if cap < max {
            tracing::trace!("now have {cap} of capacity: {max}");

            Ok(cap)
        } else if cap == max {
            tracing::trace!("at max capacity: {max}");

            Ok(cap)
        } else {
            self.wake_queue.push(waker.clone());
            tracing::debug!("now have {} pending wakers...", self.wake_queue.len());

            self.cur_capacity.fetch_sub(1, Ordering::SeqCst);
            tracing::debug!("at over capacity: {cap} of {max}");

            Err(cap)
        }
    }
}

impl<F> futures::Sink<IDiskMessage> for DiskManagerSink<F>
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    type Error = std::io::Error;

    fn poll_ready(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.try_submit_work(cx.waker()) {
            Ok(_remaining) => Poll::Ready(Ok(())),
            Err(_full) => Poll::Pending,
        }
    }

    fn start_send(self: std::pin::Pin<&mut Self>, item: IDiskMessage) -> Result<(), Self::Error> {
        tracing::trace!("Starting Send For DiskManagerSink With IDiskMessage");
        self.task_set
            .lock()
            .unwrap()
            .spawn(tasks::execute(item, self.context.clone()));
        Ok(())
    }

    fn poll_flush(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let Ok(mut task_set) = self.task_set.try_lock() else {
            tracing::warn!("unable to get task_set lock");
            cx.waker().wake_by_ref();
            return Poll::Pending;
        };

        tracing::debug!("flushing the {} tasks", task_set.len());

        while let Some(ready) = match task_set.poll_join_next(cx) {
            Poll::Ready(ready) => ready,
            Poll::Pending => {
                tracing::debug!("all {} task(s) are still pending...", task_set.len());
                return Poll::Pending;
            }
        } {
            match ready {
                Ok(()) => {
                    tracing::trace!("task completed... with {} remaining...", task_set.len());

                    continue;
                }
                Err(e) => {
                    tracing::error!("task completed... with {} remaining, with error: {e}", task_set.len());

                    return Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::Other, e)));
                }
            }
        }

        Poll::Ready(Ok(()))
    }

    fn poll_close(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_flush(cx)
    }
}
