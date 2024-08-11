use std::cell::Cell;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::future::Future;

use crate::filter::filters::Filters;
use crate::handshake::handler;
use crate::handshake::handler::HandshakeType;

#[allow(clippy::module_name_repetitions)]
pub struct ListenerHandler<S> {
    opt_item: Cell<std::io::Result<Option<HandshakeType<S>>>>,
}

impl<S> ListenerHandler<S> {
    pub fn new(item: std::io::Result<(S, SocketAddr)>, context: &Filters) -> ListenerHandler<S> {
        let (sock, addr) = match item {
            Ok(item) => item,
            Err(e) => return ListenerHandler { opt_item: Err(e).into() },
        };

        let opt_item = if handler::should_filter(Some(&addr), None, None, None, None, context) {
            None
        } else {
            Some(HandshakeType::Complete(sock, addr))
        };

        ListenerHandler {
            opt_item: Ok(opt_item).into(),
        }
    }
}

impl<S> Future for ListenerHandler<S> {
    type Output = std::io::Result<Option<HandshakeType<S>>>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(self.opt_item.replace(Ok(None)))
    }
}

#[cfg(test)]
mod tests {

    use super::ListenerHandler;
    use crate::filter::filters::test_filters::{BlockAddrFilter, BlockProtocolFilter};
    use crate::filter::filters::Filters;
    use crate::handshake::handler::HandshakeType;
    use crate::message::protocol::Protocol;

    #[tokio::test]
    async fn positive_empty_filter() {
        let exp_item = ("Testing", "0.0.0.0:0".parse().unwrap());
        let handler = ListenerHandler::new(Ok(exp_item), &Filters::new());

        let recv_enum_item = handler.await.unwrap().unwrap();

        let recv_item = match recv_enum_item {
            HandshakeType::Complete(sock, addr) => (sock, addr),
            HandshakeType::Initiate(_, _) => panic!("Expected HandshakeType::Complete"),
        };

        assert_eq!(exp_item, recv_item);
    }

    #[tokio::test]
    async fn positive_passes_filter() {
        let filters = Filters::new();
        filters.add_filter(BlockAddrFilter::new("1.2.3.4:5".parse().unwrap()));

        let exp_item = ("Testing", "0.0.0.0:0".parse().unwrap());
        let handler = ListenerHandler::new(Ok(exp_item), &filters);

        let recv_enum_item = handler.await.unwrap().unwrap();

        let recv_item = match recv_enum_item {
            HandshakeType::Complete(sock, addr) => (sock, addr),
            HandshakeType::Initiate(_, _) => panic!("Expected HandshakeType::Complete"),
        };

        assert_eq!(exp_item, recv_item);
    }

    #[tokio::test]
    async fn positive_needs_data_filter() {
        let filters = Filters::new();
        filters.add_filter(BlockProtocolFilter::new(Protocol::BitTorrent));

        let exp_item = ("Testing", "0.0.0.0:0".parse().unwrap());
        let handler = ListenerHandler::new(Ok(exp_item), &filters);

        let recv_enum_item = handler.await.unwrap().unwrap();

        let recv_item = match recv_enum_item {
            HandshakeType::Complete(sock, addr) => (sock, addr),
            HandshakeType::Initiate(_, _) => panic!("Expected HandshakeType::Complete"),
        };

        assert_eq!(exp_item, recv_item);
    }

    #[tokio::test]
    async fn positive_fails_filter() {
        let filters = Filters::new();
        filters.add_filter(BlockAddrFilter::new("0.0.0.0:0".parse().unwrap()));

        let exp_item = ("Testing", "0.0.0.0:0".parse().unwrap());
        let handler = ListenerHandler::new(Ok(exp_item), &filters);

        let recv_enum_item = handler.await.unwrap();

        if let Some(HandshakeType::Complete(_, _) | HandshakeType::Initiate(_, _)) = recv_enum_item {
            panic!("Expected No HandshakeType")
        }
    }
}
