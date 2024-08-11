use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use crossbeam::queue::SegQueue;
use futures::sync::mpsc;
use futures::{Poll, Sink, StartSend, Stream};
use sink::DiskManagerSink;
use stream::DiskManagerStream;

use crate::disk::fs::FileSystem;
use crate::disk::tasks::context::DiskManagerContext;
use crate::disk::{IDiskMessage, ODiskMessage};
use crate::DiskManagerBuilder;

pub mod builder;
pub mod sink;
pub mod stream;

/// `DiskManager` object which handles the storage of `Blocks` to the `FileSystem`.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct DiskManager<F> {
    sink: DiskManagerSink<F>,
    stream: DiskManagerStream,
}

impl<F> DiskManager<F> {
    /// Create a `DiskManager` from the given `DiskManagerBuilder`.
    pub fn from_builder(mut builder: DiskManagerBuilder, fs: F) -> DiskManager<F> {
        let cur_sink_capacity = Arc::new(AtomicUsize::new(0));
        let sink_capacity = builder.sink_buffer_capacity();
        let stream_capacity = builder.stream_buffer_capacity();
        let pool_builder = builder.worker_config();

        let (out_send, out_recv) = mpsc::channel(stream_capacity);
        let context = DiskManagerContext::new(out_send, fs);
        let task_queue = Arc::new(SegQueue::new());

        let sink = DiskManagerSink::new(
            pool_builder.create(),
            context,
            sink_capacity,
            cur_sink_capacity.clone(),
            task_queue.clone(),
        );
        let stream = DiskManagerStream::new(out_recv, cur_sink_capacity, task_queue.clone());

        DiskManager { sink, stream }
    }

    /// Break the `DiskManager` into a sink and stream.
    ///
    /// The returned sink implements `Clone`.
    #[must_use]
    pub fn into_parts(self) -> (DiskManagerSink<F>, DiskManagerStream) {
        (self.sink, self.stream)
    }
}

impl<F> Sink for DiskManager<F>
where
    F: FileSystem + Send + Sync + 'static,
{
    type SinkItem = IDiskMessage;
    type SinkError = ();

    fn start_send(&mut self, item: IDiskMessage) -> StartSend<IDiskMessage, ()> {
        self.sink.start_send(item)
    }

    fn poll_complete(&mut self) -> Poll<(), ()> {
        self.sink.poll_complete()
    }
}

impl<F> Stream for DiskManager<F> {
    type Item = ODiskMessage;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<ODiskMessage>, ()> {
        self.stream.poll()
    }
}
