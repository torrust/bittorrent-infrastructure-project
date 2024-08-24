use std::net::SocketAddr;
use std::pin::Pin;

use futures::sink::SinkExt;
use futures::stream::StreamExt;
use util::bt::{InfoHash, PeerId};

use crate::filter::filters::Filters;
use crate::filter::FilterDecision;
use crate::message::extensions::Extensions;
use crate::message::initiate::InitiateMessage;
use crate::message::protocol::Protocol;

pub mod handshaker;
pub mod initiator;
pub mod listener;

pub enum HandshakeType<S> {
    Initiate(S, InitiateMessage),
    Complete(S, SocketAddr),
}

/// Create loop for feeding the handler with the items coming from the stream, and forwarding the result to the sink.
///
/// If the stream is used up, or an error is propagated from any of the elements, the loop will terminate.
#[allow(clippy::module_name_repetitions)]
pub async fn loop_handler<M, H, K, F, R, C>(mut stream: M, mut handler: H, mut sink: K, context: Pin<Box<C>>)
where
    M: futures::Stream + Unpin,
    H: for<'a> FnMut(M::Item, &'a C) -> F,
    K: futures::Sink<std::io::Result<R>> + Unpin,
    F: futures::Future<Output = std::io::Result<Option<R>>>,
    C: 'static,
{
    while let Some(item) = stream.next().await {
        let Ok(maybe_result) = handler(item, &context).await else {
            break;
        };

        drop(match maybe_result {
            Some(result) => sink.send(Ok(result)).await,
            None => continue,
        });
    }
}

/// Computes whether or not we should filter given the parameters and filters.
pub fn should_filter(
    addr: Option<&SocketAddr>,
    prot: Option<&Protocol>,
    ext: Option<&Extensions>,
    hash: Option<&InfoHash>,
    pid: Option<&PeerId>,
    filters: &Filters,
) -> bool {
    // Initially, we set all our results to pass
    let mut addr_filter = FilterDecision::Pass;
    let mut prot_filter = FilterDecision::Pass;
    let mut ext_filter = FilterDecision::Pass;
    let mut hash_filter = FilterDecision::Pass;
    let mut pid_filter = FilterDecision::Pass;

    // Choose on individual fields
    filters.access_filters(|ref_filters| {
        for ref_filter in ref_filters {
            addr_filter = addr_filter.choose(ref_filter.on_addr(addr));
            prot_filter = prot_filter.choose(ref_filter.on_prot(prot));
            ext_filter = ext_filter.choose(ref_filter.on_ext(ext));
            hash_filter = hash_filter.choose(ref_filter.on_hash(hash));
            pid_filter = pid_filter.choose(ref_filter.on_pid(pid));
        }
    });

    // Choose across the results of individual fields
    addr_filter
        .choose(prot_filter)
        .choose(ext_filter)
        .choose(hash_filter)
        .choose(pid_filter)
        == FilterDecision::Block
}
