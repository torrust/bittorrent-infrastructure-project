use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::{Sink, SinkExt as _};

use super::{IUberMessage, UberDiscovery};
use crate::discovery::IDiscoveryMessage;
use crate::error::Error;
use crate::extended::ExtendedModule;
use crate::IExtendedMessage;

//----------------------------------------------------------------------//
/// `Sink` portion of the `UberModule` for sending messages.
#[allow(clippy::module_name_repetitions)]
#[derive(Clone)]
pub struct UberSink {
    pub(super) discovery: UberDiscovery,
    pub(super) extended: Option<ExtendedModule>,
}

impl UberSink {
    fn handle_message(&mut self, message: IUberMessage) -> Result<(), Error> {
        match message {
            IUberMessage::Control(control) => {
                if let Some(extended) = &mut self.extended {
                    let mut discovery = self.discovery.lock().unwrap();
                    let d_modules = discovery.as_mut_slice();
                    extended.process_message(IExtendedMessage::Control(*control.clone()), d_modules);
                }
                for discovery in self.discovery.lock().unwrap().iter_mut() {
                    Arc::get_mut(discovery)
                        .unwrap()
                        .start_send_unpin(IDiscoveryMessage::Control(*control.clone()))?;
                }
            }
            IUberMessage::Extended(extended) => {
                if let Some(ext_module) = &mut self.extended {
                    let mut discovery = self.discovery.lock().unwrap();
                    let d_modules = discovery.as_mut_slice();
                    ext_module.process_message(*extended.clone(), d_modules);
                }
            }
            IUberMessage::Discovery(discovery) => {
                for discovery_module in self.discovery.lock().unwrap().iter_mut() {
                    Arc::get_mut(discovery_module).unwrap().start_send_unpin(*discovery.clone())?;
                }
            }
        }
        Ok(())
    }

    fn poll_discovery_flush(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        for discovery in self.discovery.lock().unwrap().iter_mut() {
            match Arc::get_mut(discovery).unwrap().poll_flush_unpin(cx) {
                Poll::Ready(Ok(())) => continue,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(Error::Discovery(e))),
                Poll::Pending => {
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
            }
        }
        Poll::Ready(Ok(()))
    }
}

impl Sink<IUberMessage> for UberSink {
    type Error = Error;

    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: IUberMessage) -> Result<(), Self::Error> {
        self.handle_message(item)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_discovery_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_discovery_flush(cx)
    }
}
