//! `Sink` portion of the `Handshaker` for initiating handshakes.

use futures::sink::Sink;
use futures::sync::mpsc::{SendError, Sender};
use futures::{Poll, StartSend};
use util::bt::PeerId;

use crate::filter::filters::Filters;
use crate::{DiscoveryInfo, HandshakeFilter, HandshakeFilters, InitiateMessage};

#[allow(clippy::module_name_repetitions)]
#[derive(Clone)]
pub struct HandshakerSink {
    send: Sender<InitiateMessage>,
    port: u16,
    pid: PeerId,
    filters: Filters,
}

impl HandshakerSink {
    pub(super) fn new(send: Sender<InitiateMessage>, port: u16, pid: PeerId, filters: Filters) -> HandshakerSink {
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

impl Sink for HandshakerSink {
    type SinkItem = InitiateMessage;
    type SinkError = SendError<InitiateMessage>;

    fn start_send(&mut self, item: InitiateMessage) -> StartSend<InitiateMessage, SendError<InitiateMessage>> {
        self.send.start_send(item)
    }

    fn poll_complete(&mut self) -> Poll<(), SendError<InitiateMessage>> {
        self.send.poll_complete()
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
