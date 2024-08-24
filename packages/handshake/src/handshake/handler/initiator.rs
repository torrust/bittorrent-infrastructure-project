/// Handle the initiation of connections, which are returned as a `HandshakeType`.
#[allow(clippy::module_name_repetitions)]
use std::time::Duration;

use futures::future::{self, BoxFuture};
use futures::{FutureExt, TryFutureExt as _};

use crate::filter::filters::Filters;
use crate::handshake::handler;
use crate::handshake::handler::HandshakeType;
use crate::message::initiate::InitiateMessage;
use crate::transport::Transport;

/// Handle the initiation of connections, which are returned as a `HandshakeType`.
#[allow(clippy::module_name_repetitions)]
pub fn initiator_handler<'a, 'b, T>(
    item: InitiateMessage,
    context: &'b (T, Filters, Duration),
) -> BoxFuture<'a, std::io::Result<Option<HandshakeType<T::Socket>>>>
where
    T: Transport + Send + Sync + 'a,
    <T as Transport>::Socket: Send + Sync,
{
    let (transport, filters, timeout) = context;
    let timeout = *timeout;

    if handler::should_filter(
        Some(item.address()),
        Some(item.protocol()),
        None,
        Some(item.hash()),
        None,
        filters,
    ) {
        future::ok(None).boxed()
    } else {
        transport
            .connect(*item.address(), timeout)
            .map_ok(|s| Some(HandshakeType::Initiate(s, item)))
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use util::bt::{self, InfoHash, PeerId};

    use crate::filter::filters::test_filters::{BlockAddrFilter, BlockPeerIdFilter, BlockProtocolFilter};
    use crate::filter::filters::Filters;
    use crate::handshake::handler::HandshakeType;
    use crate::message::initiate::InitiateMessage;
    use crate::message::protocol::Protocol;
    use crate::transport::test_transports::MockTransport;

    fn any_peer_id() -> PeerId {
        [22u8; bt::PEER_ID_LEN].into()
    }

    fn any_info_hash() -> InfoHash {
        [55u8; bt::INFO_HASH_LEN].into()
    }

    #[tokio::test]
    async fn positive_empty_filter() {
        let exp_message = InitiateMessage::new(Protocol::BitTorrent, any_info_hash(), "1.2.3.4:5".parse().unwrap());

        let recv_enum_item = super::initiator_handler(
            exp_message.clone(),
            &(MockTransport, Filters::new(), Duration::from_millis(1000)),
        )
        .await
        .unwrap();
        let recv_item = match recv_enum_item {
            Some(HandshakeType::Initiate(_, msg)) => msg,
            Some(HandshakeType::Complete(_, _)) | None => panic!("Expected HandshakeType::Initiate"),
        };

        assert_eq!(exp_message, recv_item);
    }

    #[tokio::test]
    async fn positive_passes_filter() {
        let filters = Filters::new();
        filters.add_filter(BlockAddrFilter::new("2.3.4.5:6".parse().unwrap()));

        let exp_message = InitiateMessage::new(Protocol::BitTorrent, any_info_hash(), "1.2.3.4:5".parse().unwrap());

        let recv_enum_item =
            super::initiator_handler(exp_message.clone(), &(MockTransport, filters, Duration::from_millis(1000)))
                .await
                .unwrap();
        let recv_item = match recv_enum_item {
            Some(HandshakeType::Initiate(_, msg)) => msg,
            Some(HandshakeType::Complete(_, _)) | None => panic!("Expected HandshakeType::Initiate"),
        };

        assert_eq!(exp_message, recv_item);
    }

    #[tokio::test]
    async fn positive_needs_data_filter() {
        let filters = Filters::new();
        filters.add_filter(BlockPeerIdFilter::new(any_peer_id()));

        let exp_message = InitiateMessage::new(Protocol::BitTorrent, any_info_hash(), "1.2.3.4:5".parse().unwrap());

        let recv_enum_item =
            super::initiator_handler(exp_message.clone(), &(MockTransport, filters, Duration::from_millis(1000)))
                .await
                .unwrap();
        let recv_item = match recv_enum_item {
            Some(HandshakeType::Initiate(_, msg)) => msg,
            Some(HandshakeType::Complete(_, _)) | None => panic!("Expected HandshakeType::Initiate"),
        };

        assert_eq!(exp_message, recv_item);
    }

    #[tokio::test]
    async fn positive_fails_filter() {
        let filters = Filters::new();
        filters.add_filter(BlockProtocolFilter::new(Protocol::Custom(vec![1, 2, 3, 4])));

        let exp_message = InitiateMessage::new(
            Protocol::Custom(vec![1, 2, 3, 4]),
            any_info_hash(),
            "1.2.3.4:5".parse().unwrap(),
        );

        let recv_enum_item =
            super::initiator_handler(exp_message.clone(), &(MockTransport, filters, Duration::from_millis(1000)))
                .await
                .unwrap();
        match recv_enum_item {
            None => (),
            Some(HandshakeType::Initiate(_, _) | HandshakeType::Complete(_, _)) => panic!("Expected No Handshake"),
        }
    }
}
