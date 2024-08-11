use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::future::BoxFuture;
use futures::{Future, FutureExt as _, Stream, TryFutureExt as _};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};

use crate::local_addr::LocalAddr;

/// Trait for initializing connections over an abstract `Transport`.
pub trait Transport {
    /// The type of socket used by this transport.
    type Socket: AsyncRead + AsyncWrite + Unpin + 'static;

    /// The future that resolves to a `Socket`.
    type FutureSocket: Future<Output = std::io::Result<Self::Socket>> + Send + 'static;

    /// The type of listener used by this transport.
    type Listener: Stream<Item = std::io::Result<(Self::Socket, SocketAddr)>> + LocalAddr + Unpin + 'static;

    /// The future that resolves to a `Listener`.
    type FutureListener: Future<Output = std::io::Result<Self::Listener>> + Send + 'static;

    /// Connect to the given address using this transport.
    ///
    /// # Errors
    ///
    /// Returns an IO error if unable to connect to the socket.
    fn connect(&self, addr: SocketAddr, timeout: Duration) -> Self::FutureSocket;

    /// Listen on the given address using this transport.
    ///
    /// # Errors
    ///
    /// Returns an IO error if unable to bind to the socket.
    fn listen(&self, addr: SocketAddr, timeout: Duration) -> Self::FutureListener;
}

//----------------------------------------------------------------------------------//

/// A `Transport` implementation for TCP.
#[allow(clippy::module_name_repetitions)]
pub struct TcpTransport;

impl Transport for TcpTransport {
    type Socket = TcpStream;
    type FutureSocket = BoxFuture<'static, std::io::Result<Self::Socket>>;
    type Listener = TcpListenerStream;
    type FutureListener = BoxFuture<'static, std::io::Result<Self::Listener>>;

    fn connect(&self, addr: SocketAddr, timeout: Duration) -> Self::FutureSocket {
        let socket = TcpStream::connect(addr);
        let socket = tokio::time::timeout(timeout, socket)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::TimedOut, e))
            .boxed();

        socket.map(|s| s.and_then(|s| s)).boxed()
    }

    fn listen(&self, addr: SocketAddr, timeout: Duration) -> Self::FutureListener {
        let listener = TcpListener::bind(addr);

        let listener = tokio::time::timeout(timeout, listener)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::TimedOut, e))
            .boxed();

        let listener = listener.map(|l| l.and_then(|l| l)).boxed();

        listener.map_ok(TcpListenerStream::new).boxed()
    }
}

//----------------------------------------------------------------------------------//

/// A custom stream for `TcpListener`.
pub struct TcpListenerStream {
    listener: TcpListener,
}

impl TcpListenerStream {
    /// Creates a new `TcpListenerStream` from a `TcpListener`.
    fn new(listener: TcpListener) -> Self {
        TcpListenerStream { listener }
    }
}

impl LocalAddr for TcpListenerStream {
    fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }
}

impl Stream for TcpListenerStream {
    type Item = std::io::Result<(TcpStream, SocketAddr)>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let listener = &self.listener;
        match listener.poll_accept(cx) {
            Poll::Ready(Ok((socket, addr))) => Poll::Ready(Some(Ok((socket, addr)))),
            Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e))),
            Poll::Pending => Poll::Pending,
        }
    }
}

//----------------------------------------------------------------------------------//

#[cfg(test)]
pub mod test_transports {

    use std::net::SocketAddr;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use std::time::Duration;

    use futures::future::{self, BoxFuture};
    use futures::io::{self};
    use futures::stream::{self, Empty, Stream};
    use futures::{FutureExt as _, StreamExt as _};

    use super::Transport;
    use crate::LocalAddr;

    /// A mock transport for testing purposes.
    pub struct MockTransport;

    impl Transport for MockTransport {
        type Socket = std::io::Cursor<Vec<u8>>;
        type FutureSocket = BoxFuture<'static, std::io::Result<Self::Socket>>;
        type Listener = MockListener;
        type FutureListener = BoxFuture<'static, std::io::Result<Self::Listener>>;

        fn connect(&self, _addr: SocketAddr, _timeout: Duration) -> Self::FutureSocket {
            future::ok(std::io::Cursor::new(Vec::new())).boxed()
        }

        fn listen(&self, addr: SocketAddr, _timeout: Duration) -> Self::FutureListener {
            future::ok(MockListener::new(addr)).boxed()
        }
    }

    //----------------------------------------------------------------------------------//

    /// A mock listener for testing purposes.
    pub struct MockListener {
        addr: SocketAddr,
        empty: Empty<io::Result<(std::io::Cursor<Vec<u8>>, SocketAddr)>>,
    }

    impl MockListener {
        /// Creates a new `MockListener` with the given address.
        fn new(addr: SocketAddr) -> MockListener {
            MockListener {
                addr,
                empty: stream::empty(),
            }
        }
    }

    impl LocalAddr for MockListener {
        fn local_addr(&self) -> std::io::Result<SocketAddr> {
            Ok(self.addr)
        }
    }

    impl Stream for MockListener {
        type Item = std::io::Result<(std::io::Cursor<Vec<u8>>, SocketAddr)>;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.get_mut().empty.poll_next_unpin(cx)
        }
    }

    //----------------------------------------------------------------------------------//

    #[tokio::test]
    async fn test_mock_transport_connect() {
        let transport = MockTransport;
        let addr = "127.0.0.1:8080".parse().unwrap();
        let timeout = Duration::from_secs(1);

        let socket = transport.connect(addr, timeout).await;
        assert!(socket.is_ok());
    }

    #[tokio::test]
    async fn test_mock_transport_listen() {
        let transport = MockTransport;
        let addr = "127.0.0.1:8080".parse().unwrap();
        let timeout = Duration::from_secs(1);

        let listener = transport.listen(addr, timeout).await;
        assert!(listener.is_ok());

        let listener = listener.unwrap();
        assert_eq!(listener.local_addr().unwrap(), addr);
    }
}
