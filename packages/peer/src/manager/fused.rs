use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::stream::{Fuse, Stream};
use futures::{StreamExt, TryStream};
use tokio::time::Instant;

/// Error type for `PersistentStream`.
pub enum PersistentError<Err> {
    Disconnect,
    StreamError(Err),
}

/// Error type for `RecurringTimeoutStream`.
pub enum RecurringTimeoutError<Err> {
    Disconnect,
    Timeout,
    StreamError(Err),
}

/// A stream wrapper that ensures persistent connections. If the underlying stream yields `None`,
/// it is treated as an error, and subsequent polls will continue to return this error.
pub struct PersistentStream<St, Ty, Err>
where
    St: Stream<Item = Result<Ty, Err>>,
    St: TryStream<Ok = Ty, Error = Err>,
{
    stream: Fuse<St>,
}

impl<St, Ty, Err> PersistentStream<St, Ty, Err>
where
    St: Stream<Item = Result<Ty, Err>>,
    St: TryStream<Ok = Ty, Error = Err>,
{
    /// Creates a new `PersistentStream`.
    pub fn new(stream: St) -> PersistentStream<St, Ty, Err> {
        PersistentStream { stream: stream.fuse() }
    }
}

impl<St, Ty, Err> Stream for PersistentStream<St, Ty, Err>
where
    St: Stream<Item = Result<Ty, Err>>,
    St: TryStream<Ok = Ty, Error = Err> + Unpin,
{
    type Item = Result<Ty, PersistentError<Err>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let ready = match self.stream.poll_next_unpin(cx) {
            Poll::Ready(ready) => ready,
            Poll::Pending => return Poll::Pending,
        };

        let Some(item) = ready else {
            return Poll::Ready(Some(Err(PersistentError::Disconnect)));
        };

        match item {
            Ok(message) => Poll::Ready(Some(Ok(message))),
            Err(err) => Poll::Ready(Some(Err(PersistentError::StreamError(err)))),
        }
    }
}

//----------------------------------------------------------------------------//

/// A stream wrapper that enforces a recurring timeout. If the underlying stream does not yield
/// an item within the specified duration, a timeout error is returned.
pub struct RecurringTimeoutStream<St, Ty, Err>
where
    St: Stream<Item = Result<Ty, Err>>,
    St: TryStream<Ok = Ty, Error = Err>,
{
    stream: Fuse<St>,
    timeout: Duration,
    deadline: Instant,
}

impl<St, Ty, Err> RecurringTimeoutStream<St, Ty, Err>
where
    St: Stream<Item = Result<Ty, Err>>,
    St: TryStream<Ok = Ty, Error = Err>,
{
    /// Creates a new `RecurringTimeoutStream`.
    pub fn new(stream: St, timeout: Duration) -> RecurringTimeoutStream<St, Ty, Err> {
        RecurringTimeoutStream {
            stream: stream.fuse(),
            timeout,
            deadline: Instant::now() + timeout,
        }
    }
}

impl<St, Ty, Err> Stream for RecurringTimeoutStream<St, Ty, Err>
where
    St: Stream<Item = Result<Ty, Err>>,
    St: TryStream<Ok = Ty, Error = Err> + Unpin,
{
    type Item = Result<Ty, RecurringTimeoutError<Err>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let ready = match self.stream.poll_next_unpin(cx) {
            Poll::Ready(ready) => ready,
            Poll::Pending => {
                let now = Instant::now();
                if now > self.deadline {
                    self.deadline = now + self.timeout;

                    return Poll::Ready(Some(Err(RecurringTimeoutError::Timeout)));
                }
                return Poll::Pending;
            }
        };

        let Some(item) = ready else {
            return Poll::Ready(Some(Err(RecurringTimeoutError::Disconnect)));
        };

        match item {
            Ok(message) => {
                // Reset the timeout
                self.deadline = Instant::now() + self.timeout;

                Poll::Ready(Some(Ok(message)))
            }
            Err(err) => Poll::Ready(Some(Err(RecurringTimeoutError::StreamError(err)))),
        }
    }
}
