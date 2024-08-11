//! Stream half of a `PeerManager`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crossbeam::queue::SegQueue;
use futures::sink::Sink;
use futures::stream::Stream;
use futures::sync::mpsc::{Receiver, Sender};
use futures::task::{self as futures_task, Task};
use futures::{Async, Poll};

use super::messages::{IPeerManagerMessage, OPeerManagerMessage};
use crate::manager::peer_info::PeerInfo;

#[allow(clippy::module_name_repetitions)]
#[allow(clippy::option_option)]
pub struct PeerManagerStream<P>
where
    P: Sink + Stream,
{
    recv: Receiver<OPeerManagerMessage<P::Item>>,
    peers: Arc<Mutex<HashMap<PeerInfo, Sender<IPeerManagerMessage<P>>>>>,
    task_queue: Arc<SegQueue<Task>>,
    opt_pending: Option<Option<OPeerManagerMessage<P::Item>>>,
}

impl<P> PeerManagerStream<P>
where
    P: Sink + Stream,
{
    pub(super) fn new(
        recv: Receiver<OPeerManagerMessage<P::Item>>,
        peers: Arc<Mutex<HashMap<PeerInfo, Sender<IPeerManagerMessage<P>>>>>,
        task_queue: Arc<SegQueue<Task>>,
    ) -> PeerManagerStream<P> {
        PeerManagerStream {
            recv,
            peers,
            task_queue,
            opt_pending: None,
        }
    }

    fn run_with_lock_poll<F, T, E, I, G>(&mut self, item: I, call: F, not: G) -> Poll<T, E>
    where
        F: FnOnce(I, &mut HashMap<PeerInfo, Sender<IPeerManagerMessage<P>>>) -> Poll<T, E>,
        G: FnOnce(I) -> Option<OPeerManagerMessage<P::Item>>,
    {
        let (result, took_lock) = if let Ok(mut guard) = self.peers.try_lock() {
            let result = call(item, &mut *guard);

            // Nothing calling us will return NotReady, so we don't have to push to queue here

            (result, true)
        } else {
            // Couldn't get the lock, stash a task away
            self.task_queue.push(futures_task::current());

            // Try to get the lock once more, in case of a race condition with stashing the task
            if let Ok(mut guard) = self.peers.try_lock() {
                let result = call(item, &mut *guard);

                // Nothing calling us will return NotReady, so we don't have to push to queue here

                (result, true)
            } else {
                // If we couldn't get the lock, stash the item
                self.opt_pending = Some(not(item));

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

impl<P> Stream for PeerManagerStream<P>
where
    P: Sink + Stream,
{
    type Item = OPeerManagerMessage<P::Item>;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        // Intercept and propagate any messages indicating the peer shutdown so we can remove them from our peer map
        let next_message = self
            .opt_pending
            .take()
            .map(|pending| Ok(Async::Ready(pending)))
            .unwrap_or_else(|| self.recv.poll());

        next_message.and_then(|result| match result {
            Async::Ready(Some(OPeerManagerMessage::PeerRemoved(info))) => self.run_with_lock_poll(
                info,
                |info, peers| {
                    peers
                        .remove(&info)
                        .unwrap_or_else(|| panic!("bip_peer: Received PeerRemoved Message With No Matching Peer In Map"));

                    Ok(Async::Ready(Some(OPeerManagerMessage::PeerRemoved(info))))
                },
                |info| Some(OPeerManagerMessage::PeerRemoved(info)),
            ),
            Async::Ready(Some(OPeerManagerMessage::PeerDisconnect(info))) => self.run_with_lock_poll(
                info,
                |info, peers| {
                    peers
                        .remove(&info)
                        .unwrap_or_else(|| panic!("bip_peer: Received PeerDisconnect Message With No Matching Peer In Map"));

                    Ok(Async::Ready(Some(OPeerManagerMessage::PeerDisconnect(info))))
                },
                |info| Some(OPeerManagerMessage::PeerDisconnect(info)),
            ),
            Async::Ready(Some(OPeerManagerMessage::PeerError(info, error))) => self.run_with_lock_poll(
                (info, error),
                |(info, error), peers| {
                    peers
                        .remove(&info)
                        .unwrap_or_else(|| panic!("bip_peer: Received PeerError Message With No Matching Peer In Map"));

                    Ok(Async::Ready(Some(OPeerManagerMessage::PeerError(info, error))))
                },
                |(info, error)| Some(OPeerManagerMessage::PeerError(info, error)),
            ),
            other => Ok(other),
        })
    }
}
