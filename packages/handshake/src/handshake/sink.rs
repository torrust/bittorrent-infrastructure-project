//! `Sink` portion of the `Handshaker` for initiating handshakes.

use futures::channel::mpsc;
use futures::sink::Sink;
use futures::task::{Context, Poll};
use futures::SinkExt as _;
use util::bt::PeerId;

use crate::discovery::DiscoveryInfo;
use crate::filter::filters::Filters;
use crate::filter::{HandshakeFilter, HandshakeFilters};
use crate::message::initiate::InitiateMessage;

#[allow(clippy::module_name_repetitions)]
#[derive(Clone)]
pub struct HandshakerSink {
    send: mpsc::Sender<InitiateMessage>,
    port: u16,
    pid: PeerId,
    filters: Filters,
}

impl HandshakerSink {
    pub(super) fn new(send: mpsc::Sender<InitiateMessage>, port: u16, pid: PeerId, filters: Filters) -> HandshakerSink {
        HandshakerSink {
            send,
            port,
            pid,
            filters,
        }
    }
}

impl DiscoveryInfo for HandshakerSink {
    fn port(&self) -> u16 {
        self.port
    }

    fn peer_id(&self) -> PeerId {
        self.pid
    }
}

impl Sink<InitiateMessage> for HandshakerSink {
    type Error = mpsc::SendError;

    fn poll_ready(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.send.poll_ready_unpin(cx)
    }

    fn start_send(mut self: std::pin::Pin<&mut Self>, item: InitiateMessage) -> Result<(), Self::Error> {
        self.send.start_send_unpin(item)
    }

    fn poll_flush(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.send.poll_flush_unpin(cx)
    }

    fn poll_close(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.send.poll_close_unpin(cx)
    }
}

impl HandshakeFilters for HandshakerSink {
    fn add_filter<F>(&self, filter: F)
    where
        F: HandshakeFilter + PartialEq + Eq + Send + Sync + 'static,
    {
        self.filters.add_filter(filter);
    }

    fn remove_filter<F>(&self, filter: F)
    where
        F: HandshakeFilter + PartialEq + Eq + Send + Sync + 'static,
    {
        self.filters.remove_filter(&filter);
    }

    fn clear_filters(&self) {
        self.filters.clear_filters();
    }
}

//----------------------------------------------------------------------------------//
