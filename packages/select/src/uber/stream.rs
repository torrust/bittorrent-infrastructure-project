use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::{Stream, StreamExt as _};

use super::{OUberMessage, UberDiscovery};
use crate::error::Error;
use crate::extended::ExtendedModule;

//----------------------------------------------------------------------//
/// `Stream` portion of the `UberModule` for receiving messages.
#[allow(clippy::module_name_repetitions)]
pub struct UberStream {
    pub(super) discovery: UberDiscovery,
    pub(super) extended: Option<ExtendedModule>,
}

impl Stream for UberStream {
    type Item = Result<OUberMessage, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(extended) = &mut self.extended {
            match extended.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(message))) => return Poll::Ready(Some(Ok(OUberMessage::Extended(message)))),
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => (),
            }
        };

        for discovery in self.discovery.lock().unwrap().iter_mut() {
            match Arc::get_mut(discovery).unwrap().poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(message))) => return Poll::Ready(Some(Ok(OUberMessage::Discovery(message)))),
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(Error::Discovery(e)))),
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => (),
            }
        }

        cx.waker().wake_by_ref();
        Poll::Pending
    }
}
