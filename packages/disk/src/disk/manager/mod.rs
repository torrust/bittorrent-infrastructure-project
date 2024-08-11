//! `DiskManager` object which handles the storage of `Blocks` to the `FileSystem`.

use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::task::{Context, Poll};

use crossbeam::queue::SegQueue;
use futures::channel::mpsc;
use futures::Stream;
use pin_project::pin_project;
pub use sink::DiskManagerSink;
pub use stream::DiskManagerStream;

use super::tasks::context::DiskManagerContext;
use super::{IDiskMessage, ODiskMessage};
use crate::{DiskManagerBuilder, FileSystem};

pub mod builder;
pub mod sink;
pub mod stream;

#[allow(clippy::module_name_repetitions)]
#[pin_project]
#[derive(Debug)]
pub struct DiskManager<F>
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    #[pin]
    sink: DiskManagerSink<F>,
    #[pin]
    stream: DiskManagerStream,
}

impl<F> DiskManager<F>
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    /// Create a `DiskManager` from the given `DiskManagerBuilder`.
    pub fn from_builder(builder: &DiskManagerBuilder, fs: Arc<F>) -> DiskManager<F> {
        let cur_sink_capacity = Arc::new(AtomicUsize::new(0));
        let sink_capacity = builder.sink_buffer_capacity();
        let stream_capacity = builder.stream_buffer_capacity();

        let (out_send, out_recv) = mpsc::channel(stream_capacity);
        let context = DiskManagerContext::new(out_send, fs);
        let wake_queue = Arc::new(SegQueue::new());

        let sink = DiskManagerSink::new(context, sink_capacity, cur_sink_capacity.clone(), wake_queue.clone());
        let stream = DiskManagerStream::new(out_recv, cur_sink_capacity, wake_queue.clone());

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

impl<F> futures::Sink<IDiskMessage> for DiskManager<F>
where
    F: FileSystem + Send + Sync + 'static,
{
    type Error = std::io::Error;

    fn poll_ready(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().sink.poll_ready(cx)
    }

    fn start_send(self: std::pin::Pin<&mut Self>, item: IDiskMessage) -> Result<(), Self::Error> {
        self.project().sink.start_send(item)
    }

    fn poll_flush(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().sink.poll_flush(cx)
    }

    fn poll_close(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().sink.poll_close(cx)
    }
}

impl<F> Stream for DiskManager<F>
where
    F: FileSystem + Sync + 'static,
    Arc<F>: Send + Sync,
{
    type Item = Result<ODiskMessage, ()>;

    fn poll_next(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().stream.poll_next(cx)
    }
}
