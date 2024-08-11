use std::any::Any;
use std::time::Duration;

use common::{tracing_stderr_init, INIT};
use futures::sink::SinkExt;
use futures::stream::{self, StreamExt};
use futures::FutureExt as _;
use handshake::transports::TcpTransport;
use handshake::{DiscoveryInfo, FilterDecision, HandshakeFilter, HandshakeFilters, HandshakerBuilder, InitiateMessage, Protocol};
use tracing::level_filters::LevelFilter;
use util::bt::{self, InfoHash};

mod common;

#[derive(PartialEq, Eq)]
pub struct FilterBlockAllHash;

impl HandshakeFilter for FilterBlockAllHash {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn on_hash(&self, _opt_hash: Option<&InfoHash>) -> FilterDecision {
        FilterDecision::Block
    }
}

#[derive(PartialEq, Eq)]
pub struct FilterAllowHash {
    hash: InfoHash,
}

impl HandshakeFilter for FilterAllowHash {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn on_hash(&self, opt_hash: Option<&InfoHash>) -> FilterDecision {
        opt_hash.map_or(FilterDecision::NeedData, |hash| {
            if hash == &self.hash {
                FilterDecision::Allow
            } else {
                FilterDecision::Pass
            }
        })
    }
}

#[tokio::test]
async fn test_filter_whitelist_diff_data() {
    INIT.call_once(|| {
        tracing_stderr_init(LevelFilter::INFO);
    });

    let handshaker_one_addr = "127.0.0.1:0".parse().unwrap();
    let handshaker_one_pid = [4u8; bt::PEER_ID_LEN].into();

    let (handshaker_one, mut tasks_one) = HandshakerBuilder::new()
        .with_bind_addr(handshaker_one_addr)
        .with_peer_id(handshaker_one_pid)
        .build(TcpTransport)
        .await
        .unwrap();

    let mut handshaker_one_addr = handshaker_one_addr;
    handshaker_one_addr.set_port(handshaker_one.port());
    // Filter all incoming handshake requests hash's, then whitelist
    let allow_info_hash = [55u8; bt::INFO_HASH_LEN].into();

    handshaker_one.add_filter(FilterBlockAllHash);
    handshaker_one.add_filter(FilterAllowHash { hash: allow_info_hash });

    let handshaker_two_addr = "127.0.0.1:0".parse().unwrap();
    let handshaker_two_pid = [5u8; bt::PEER_ID_LEN].into();

    let (handshaker_two, mut tasks_two) = HandshakerBuilder::new()
        .with_bind_addr(handshaker_two_addr)
        .with_peer_id(handshaker_two_pid)
        .build(TcpTransport)
        .await
        .unwrap();

    let mut handshaker_two_addr = handshaker_two_addr;
    handshaker_two_addr.set_port(handshaker_two.port());

    let (_, stream_one) = handshaker_one.into_parts();
    let (mut sink_two, stream_two) = handshaker_two.into_parts();

    let test = tokio::spawn(async move {
        // Send the initiate message
        sink_two
            .send(InitiateMessage::new(
                Protocol::BitTorrent,
                [54u8; bt::INFO_HASH_LEN].into(),
                handshaker_one_addr,
            ))
            .await
            .unwrap();

        // Use tokio timeout to wait for the result
        let get_handshake = async move {
            let mut merged = stream::select(stream_one, stream_two);
            loop {
                tokio::time::sleep(Duration::from_millis(5)).await;

                let Some(res) = merged.next().now_or_never() else {
                    continue;
                };
                break res;
            }
        };

        let res = tokio::time::timeout(Duration::from_millis(50), get_handshake).await;

        if let Ok(item) = res {
            panic!("expected timeout, but got a result: {item:?}");
        } else {
            tracing::debug!("timeout was reached");
        }
    });

    let res = test.await;

    tasks_one.shutdown().await;
    tasks_two.shutdown().await;

    res.unwrap();
}
