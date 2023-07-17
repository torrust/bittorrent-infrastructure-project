use filter::filters::Filters;
use futures::future::{self, Future};
use handshake::handler;
use handshake::handler::timer::HandshakeTimer;
use handshake::handler::HandshakeType;
use message::initiate::InitiateMessage;
use tokio_core::reactor::Handle;
use transport::Transport;

/// Handle the initiation of connections, which are returned as a HandshakeType.
pub fn initiator_handler<T>(
    item: InitiateMessage,
    context: &(T, Filters, Handle, HandshakeTimer),
) -> Box<Future<Item = Option<HandshakeType<T::Socket>>, Error = ()>>
where
    T: Transport,
{
    let &(ref transport, ref filters, ref handle, ref timer) = context;

    if handler::should_filter(
        Some(item.address()),
        Some(item.protocol()),
        None,
        Some(item.hash()),
        None,
        filters,
    ) {
        Box::new(future::ok(None))
    } else {
        let res_connect = transport
            .connect(item.address(), handle)
            .map(|connect| timer.timeout(connect));

        Box::new(
            future::lazy(|| res_connect)
                .flatten()
                .map(|socket| Some(HandshakeType::Initiate(socket, item)))
                .or_else(|_| Ok(None)),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bip_util::bt::{self, InfoHash, PeerId};
    use filter::filters::test_filters::{BlockAddrFilter, BlockPeerIdFilter, BlockProtocolFilter};
    use filter::filters::Filters;
    use futures::Future;
    use handshake::handler::timer::HandshakeTimer;
    use handshake::handler::HandshakeType;
    use message::initiate::InitiateMessage;
    use message::protocol::Protocol;
    use tokio_core::reactor::Core;
    use tokio_timer;
    use transport::test_transports::MockTransport;

    fn any_peer_id() -> PeerId {
        [22u8; bt::PEER_ID_LEN].into()
    }

    fn any_info_hash() -> InfoHash {
        [55u8; bt::INFO_HASH_LEN].into()
    }

    #[test]
    fn positive_empty_filter() {
        let core = Core::new().unwrap();
        let exp_message = InitiateMessage::new(Protocol::BitTorrent, any_info_hash(), "1.2.3.4:5".parse().unwrap());
        let timer = HandshakeTimer::new(tokio_timer::wheel().build(), Duration::from_millis(1000));

        let recv_enum_item =
            super::initiator_handler(exp_message.clone(), &(MockTransport, Filters::new(), core.handle(), timer))
                .wait()
                .unwrap();
        let recv_item = match recv_enum_item {
            Some(HandshakeType::Initiate(_, msg)) => msg,
            Some(HandshakeType::Complete(_, _)) | None => panic!("Expected HandshakeType::Initiate"),
        };

        assert_eq!(exp_message, recv_item);
    }

    #[test]
    fn positive_passes_filter() {
        let core = Core::new().unwrap();
        let timer = HandshakeTimer::new(tokio_timer::wheel().build(), Duration::from_millis(1000));

        let filters = Filters::new();
        filters.add_filter(BlockAddrFilter::new("2.3.4.5:6".parse().unwrap()));

        let exp_message = InitiateMessage::new(Protocol::BitTorrent, any_info_hash(), "1.2.3.4:5".parse().unwrap());

        let recv_enum_item = super::initiator_handler(exp_message.clone(), &(MockTransport, filters, core.handle(), timer))
            .wait()
            .unwrap();
        let recv_item = match recv_enum_item {
            Some(HandshakeType::Initiate(_, msg)) => msg,
            Some(HandshakeType::Complete(_, _)) | None => panic!("Expected HandshakeType::Initiate"),
        };

        assert_eq!(exp_message, recv_item);
    }

    #[test]
    fn positive_needs_data_filter() {
        let core = Core::new().unwrap();
        let timer = HandshakeTimer::new(tokio_timer::wheel().build(), Duration::from_millis(1000));

        let filters = Filters::new();
        filters.add_filter(BlockPeerIdFilter::new(any_peer_id()));

        let exp_message = InitiateMessage::new(Protocol::BitTorrent, any_info_hash(), "1.2.3.4:5".parse().unwrap());

        let recv_enum_item = super::initiator_handler(exp_message.clone(), &(MockTransport, filters, core.handle(), timer))
            .wait()
            .unwrap();
        let recv_item = match recv_enum_item {
            Some(HandshakeType::Initiate(_, msg)) => msg,
            Some(HandshakeType::Complete(_, _)) | None => panic!("Expected HandshakeType::Initiate"),
        };

        assert_eq!(exp_message, recv_item);
    }

    #[test]
    fn positive_fails_filter() {
        let core = Core::new().unwrap();
        let timer = HandshakeTimer::new(tokio_timer::wheel().build(), Duration::from_millis(1000));

        let filters = Filters::new();
        filters.add_filter(BlockProtocolFilter::new(Protocol::Custom(vec![1, 2, 3, 4])));

        let exp_message = InitiateMessage::new(
            Protocol::Custom(vec![1, 2, 3, 4]),
            any_info_hash(),
            "1.2.3.4:5".parse().unwrap(),
        );

        let recv_enum_item = super::initiator_handler(exp_message.clone(), &(MockTransport, filters, core.handle(), timer))
            .wait()
            .unwrap();
        match recv_enum_item {
            None => (),
            Some(HandshakeType::Initiate(_, _)) | Some(HandshakeType::Complete(_, _)) => panic!("Expected No Handshake"),
        }
    }
}
