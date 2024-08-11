use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use futures::stream::Stream;
use peer::messages::builders::ExtendedMessageBuilder;
use peer::messages::ExtendedMessage;
use peer::PeerInfo;

use crate::error::Error;
use crate::ControlMessage;

/// Enumeration of extended messages that can be sent to the extended module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IExtendedMessage {
    Control(ControlMessage),
    ReceivedExtendedMessage(PeerInfo, ExtendedMessage),
}

/// Enumeration of extended messages that can be received from the extended module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OExtendedMessage {
    SendExtendedMessage(PeerInfo, ExtendedMessage),
}

/// Trait for a module to take part in constructing the extended message for a peer.
#[allow(clippy::module_name_repetitions)]
pub trait ExtendedListener {
    /// Extend the given extended message builder for the given peer.
    fn extend(&self, _info: &PeerInfo, builder: ExtendedMessageBuilder) -> ExtendedMessageBuilder {
        builder
    }

    /// One or both sides of a peer connection had their extended information updated.
    ///
    /// This can be called multiple times for any given peer as extension information updates.
    fn on_update(&mut self, _info: &PeerInfo, _extended: &ExtendedPeerInfo) {}
}

/// Container for both the local and remote `ExtendedMessage`.
#[allow(clippy::module_name_repetitions)]
pub struct ExtendedPeerInfo {
    ours: Option<ExtendedMessage>,
    theirs: Option<ExtendedMessage>,
}

impl ExtendedPeerInfo {
    pub fn new(ours: Option<ExtendedMessage>, theirs: Option<ExtendedMessage>) -> ExtendedPeerInfo {
        ExtendedPeerInfo { ours, theirs }
    }

    pub fn update_ours(&mut self, message: ExtendedMessage) {
        self.ours = Some(message);
    }

    pub fn update_theirs(&mut self, message: ExtendedMessage) {
        self.theirs = Some(message);
    }

    pub fn our_message(&self) -> Option<&ExtendedMessage> {
        self.ours.as_ref()
    }

    pub fn their_message(&self) -> Option<&ExtendedMessage> {
        self.theirs.as_ref()
    }
}

//------------------------------------------------------------------------------//

#[allow(clippy::module_name_repetitions)]
#[derive(Clone)]
pub struct ExtendedModule {
    builder: ExtendedMessageBuilder,
    peers: Arc<Mutex<HashMap<PeerInfo, ExtendedPeerInfo>>>,
    out_queue: Arc<Mutex<VecDeque<OExtendedMessage>>>,
    opt_waker: Arc<Mutex<Option<Waker>>>,
}

impl ExtendedModule {
    pub fn new(builder: ExtendedMessageBuilder) -> ExtendedModule {
        ExtendedModule {
            builder,
            peers: Arc::default(),
            out_queue: Arc::default(),
            opt_waker: Arc::default(),
        }
    }

    pub fn process_message<D>(&self, message: IExtendedMessage, d_modules: &mut [Arc<D>])
    where
        D: ExtendedListener + ?Sized,
    {
        match message {
            IExtendedMessage::Control(ControlMessage::PeerConnected(info)) => {
                let mut builder = self.builder.clone();

                for d_module in &*d_modules {
                    let temp_builder = builder;
                    builder = d_module.extend(&info, temp_builder);
                }

                let ext_message = builder.build();
                let ext_peer_info = ExtendedPeerInfo::new(Some(ext_message.clone()), None);

                for d_module in d_modules {
                    Arc::get_mut(d_module).unwrap().on_update(&info, &ext_peer_info);
                }

                self.peers.lock().unwrap().insert(info, ext_peer_info);
                self.out_queue
                    .lock()
                    .unwrap()
                    .push_back(OExtendedMessage::SendExtendedMessage(info, ext_message));
            }
            IExtendedMessage::Control(ControlMessage::PeerDisconnected(info)) => {
                self.peers.lock().unwrap().remove(&info);
            }
            IExtendedMessage::ReceivedExtendedMessage(info, ext_message) => {
                if let Some(ext_peer_info) = self.peers.lock().unwrap().get_mut(&info) {
                    ext_peer_info.update_theirs(ext_message);

                    for d_module in d_modules {
                        Arc::get_mut(d_module).unwrap().on_update(&info, ext_peer_info);
                    }
                }
            }
            IExtendedMessage::Control(_) => (),
        }

        self.check_stream_unblock();
    }

    fn check_stream_unblock(&self) {
        if !self.out_queue.lock().unwrap().is_empty() {
            if let Some(waker) = self.opt_waker.lock().unwrap().take() {
                waker.wake();
            }
        }
    }
}

impl Stream for ExtendedModule {
    type Item = Result<OExtendedMessage, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(message) = self.out_queue.lock().unwrap().pop_front() {
            Poll::Ready(Some(Ok(message)))
        } else {
            self.opt_waker.lock().unwrap().replace(cx.waker().clone());
            Poll::Pending
        }
    }
}
