//! Sink half of a `PeerManager`.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crossbeam::queue::SegQueue;
use futures::sink::Sink;
use futures::stream::Stream;
use futures::sync::mpsc::Sender;
use futures::task::{self as futures_task, Task};
use futures::{Async, AsyncSink, Poll, StartSend};
use tokio_core::reactor::Handle;
use tokio_timer::{self, Timer};

use super::messages::{IPeerManagerMessage, ManagedMessage, OPeerManagerMessage};
use super::task;
use crate::manager::builder::PeerManagerBuilder;
use crate::manager::error::{PeerManagerError, PeerManagerErrorKind};
use crate::manager::peer_info::PeerInfo;

#[allow(clippy::module_name_repetitions)]
pub struct PeerManagerSink<P>
where
    P: Sink + Stream,
{
    handle: Handle,
    timer: Timer,
    build: PeerManagerBuilder,
    send: Sender<OPeerManagerMessage<P::Item>>,
    peers: Arc<Mutex<HashMap<PeerInfo, Sender<IPeerManagerMessage<P>>>>>,
    task_queue: Arc<SegQueue<Task>>,
}

impl<P> Clone for PeerManagerSink<P>
where
    P: Sink + Stream,
{
    fn clone(&self) -> PeerManagerSink<P> {
        PeerManagerSink {
            handle: self.handle.clone(),
            timer: self.timer.clone(),
            build: self.build,
            send: self.send.clone(),
            peers: self.peers.clone(),
            task_queue: self.task_queue.clone(),
        }
    }
}

impl<P> PeerManagerSink<P>
where
    P: Sink + Stream,
{
    pub(super) fn new(
        handle: Handle,
        timer: Timer,
        build: PeerManagerBuilder,
        send: Sender<OPeerManagerMessage<P::Item>>,
        peers: Arc<Mutex<HashMap<PeerInfo, Sender<IPeerManagerMessage<P>>>>>,
        task_queue: Arc<SegQueue<Task>>,
    ) -> PeerManagerSink<P> {
        PeerManagerSink {
            handle,
            timer,
            build,
            send,
            peers,
            task_queue,
        }
    }

    fn run_with_lock_sink<F, T, E, G, I>(&mut self, item: I, call: F, not: G) -> StartSend<T, E>
    where
        F: FnOnce(
            I,
            &mut Handle,
            &mut Timer,
            &mut PeerManagerBuilder,
            &mut Sender<OPeerManagerMessage<P::Item>>,
            &mut HashMap<PeerInfo, Sender<IPeerManagerMessage<P>>>,
        ) -> StartSend<T, E>,
        G: FnOnce(I) -> T,
    {
        let (result, took_lock) = if let Ok(mut guard) = self.peers.try_lock() {
            let result = call(
                item,
                &mut self.handle,
                &mut self.timer,
                &mut self.build,
                &mut self.send,
                &mut *guard,
            );

            // Closure could return not ready, need to stash in that case
            if result.as_ref().map(futures::AsyncSink::is_not_ready).unwrap_or(false) {
                self.task_queue.push(futures_task::current());
            }

            (result, true)
        } else {
            self.task_queue.push(futures_task::current());

            if let Ok(mut guard) = self.peers.try_lock() {
                let result = call(
                    item,
                    &mut self.handle,
                    &mut self.timer,
                    &mut self.build,
                    &mut self.send,
                    &mut *guard,
                );

                // Closure could return not ready, need to stash in that case
                if result.as_ref().map(futures::AsyncSink::is_not_ready).unwrap_or(false) {
                    self.task_queue.push(futures_task::current());
                }

                (result, true)
            } else {
                (Ok(AsyncSink::NotReady(not(item))), false)
            }
        };

        if took_lock {
            // Just notify a single person waiting on the lock to reduce contention
            if let Some(task) = self.task_queue.pop() {
                task.notify();
            }
        }

        result
    }

    fn run_with_lock_poll<F, T, E>(&mut self, call: F) -> Poll<T, E>
    where
        F: FnOnce(
            &mut Handle,
            &mut Timer,
            &mut PeerManagerBuilder,
            &mut Sender<OPeerManagerMessage<P::Item>>,
            &mut HashMap<PeerInfo, Sender<IPeerManagerMessage<P>>>,
        ) -> Poll<T, E>,
    {
        let (result, took_lock) = if let Ok(mut guard) = self.peers.try_lock() {
            let result = call(
                &mut self.handle,
                &mut self.timer,
                &mut self.build,
                &mut self.send,
                &mut *guard,
            );

            (result, true)
        } else {
            // Stash a task
            self.task_queue.push(futures_task::current());

            // Try to get lock again in case of race condition
            if let Ok(mut guard) = self.peers.try_lock() {
                let result = call(
                    &mut self.handle,
                    &mut self.timer,
                    &mut self.build,
                    &mut self.send,
                    &mut *guard,
                );

                (result, true)
            } else {
                (Ok(Async::NotReady), false)
            }
        };

        if took_lock {
            // Just notify a single person waiting on the lock to reduce contention
            if let Some(task) = self.task_queue.pop() {
                task.notify();
            }
        }

        result
    }
}

impl<P> Sink for PeerManagerSink<P>
where
    P: Sink<SinkError = std::io::Error> + Stream<Error = std::io::Error> + 'static,
    P::SinkItem: ManagedMessage,
    P::Item: ManagedMessage,
{
    type SinkItem = IPeerManagerMessage<P>;
    type SinkError = PeerManagerError;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        match item {
            IPeerManagerMessage::AddPeer(info, peer) => self.run_with_lock_sink(
                (info, peer),
                |(info, peer), handle, timer, builder, send, peers| {
                    if peers.len() >= builder.peer_capacity() {
                        Ok(AsyncSink::NotReady(IPeerManagerMessage::AddPeer(info, peer)))
                    } else {
                        match peers.entry(info) {
                            Entry::Occupied(_) => Err(PeerManagerError::from_kind(PeerManagerErrorKind::PeerNotFound { info })),
                            Entry::Vacant(vac) => {
                                vac.insert(task::run_peer(peer, info, send.clone(), timer.clone(), builder, handle));

                                Ok(AsyncSink::Ready)
                            }
                        }
                    }
                },
                |(info, peer)| IPeerManagerMessage::AddPeer(info, peer),
            ),
            IPeerManagerMessage::RemovePeer(info) => self.run_with_lock_sink(
                info,
                |info, _, _, _, _, peers| {
                    peers
                        .get_mut(&info)
                        .ok_or_else(|| PeerManagerError::from_kind(PeerManagerErrorKind::PeerNotFound { info }))
                        .and_then(|send| {
                            send.start_send(IPeerManagerMessage::RemovePeer(info))
                                .map_err(|_| panic!("bip_peer: PeerManager Failed To Send RemovePeer"))
                        })
                },
                |info| IPeerManagerMessage::RemovePeer(info),
            ),
            IPeerManagerMessage::SendMessage(info, mid, peer_message) => self.run_with_lock_sink(
                (info, mid, peer_message),
                |(info, mid, peer_message), _, _, _, _, peers| {
                    peers
                        .get_mut(&info)
                        .ok_or_else(|| PeerManagerError::from_kind(PeerManagerErrorKind::PeerNotFound { info }))
                        .and_then(|send| {
                            send.start_send(IPeerManagerMessage::SendMessage(info, mid, peer_message))
                                .map_err(|_| panic!("bip_peer: PeerManager Failed to Send SendMessage"))
                        })
                },
                |(info, mid, peer_message)| IPeerManagerMessage::SendMessage(info, mid, peer_message),
            ),
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.run_with_lock_poll(|_, _, _, _, peers| {
            for peer_mut in peers.values_mut() {
                // Needs type hint in case poll fails (so that error type matches)
                let result: Poll<(), Self::SinkError> = peer_mut
                    .poll_complete()
                    .map_err(|_| panic!("bip_peer: PeerManaged Failed To Poll Peer"));

                result?;
            }

            Ok(Async::Ready(()))
        })
    }
}
