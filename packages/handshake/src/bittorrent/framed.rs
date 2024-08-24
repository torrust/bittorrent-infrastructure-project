//! This module provides the `FramedHandshake` struct, which implements a framed transport for the `BitTorrent` handshake protocol.
//! It supports both reading from and writing to an underlying asynchronous stream, handling the framing of handshake messages.

use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Buf as _, BufMut, BytesMut};
use futures::sink::Sink;
use futures::stream::Stream;
use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tracing::instrument;

use crate::bittorrent::message::{self, HandshakeMessage};

/// Represents the state of the handshake process.
#[derive(Debug)]
enum HandshakeState {
    Waiting,
    Reading,
    Ready,
    Finished,
    Errored,
}

/// A framed transport for the `BitTorrent` handshake protocol.
///
/// This struct wraps an underlying asynchronous stream and provides methods to read and write `HandshakeMessage` instances.
#[allow(clippy::module_name_repetitions)]
#[pin_project]
#[derive(Debug)]
pub struct FramedHandshake<S>
where
    S: std::fmt::Debug + Unpin,
{
    #[pin]
    sock: S,

    write_buffer: BytesMut,
    read_buffer: Vec<u8>,
    read_pos: usize,
    state: HandshakeState,
}

impl<S> FramedHandshake<S>
where
    S: std::fmt::Debug + Unpin,
{
    /// Creates a new `FramedHandshake` with the given socket.
    ///
    /// # Arguments
    ///
    /// * `sock` - The underlying asynchronous stream.
    pub fn new(sock: S) -> FramedHandshake<S> {
        FramedHandshake {
            sock,
            write_buffer: BytesMut::with_capacity(1),
            read_buffer: Vec::default(),
            read_pos: 0,
            state: HandshakeState::Waiting,
        }
    }

    /// Consumes the `FramedHandshake`, returning the underlying socket.
    pub fn into_inner(self) -> S {
        self.sock
    }
}

impl<Si> Sink<HandshakeMessage> for FramedHandshake<Si>
where
    Si: AsyncWrite + std::fmt::Debug + Unpin,
{
    type Error = std::io::Error;

    #[instrument(skip(self, _cx))]
    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        tracing::trace!("poll_ready called");

        Poll::Ready(Ok(()))
    }

    #[instrument(skip(self))]
    fn start_send(mut self: Pin<&mut Self>, item: HandshakeMessage) -> Result<(), Self::Error> {
        tracing::trace!("start_send called with item: {item:?}");
        let mut cursor = std::io::Cursor::new(Vec::with_capacity(item.write_len()));
        item.write_bytes_sync(&mut cursor)?;

        self.write_buffer.reserve(item.write_len());
        self.write_buffer.put_slice(cursor.get_ref());

        Ok(())
    }

    #[instrument(skip(self, cx))]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        tracing::trace!("poll_flush called");

        let mut project = self.project();

        while !project.write_buffer.is_empty() {
            let res = project.sock.as_mut().poll_write(cx, project.write_buffer);

            match res {
                Poll::Ready(Ok(0)) => {
                    tracing::error!("Failed to write bytes: WriteZero");
                    return Err(std::io::Error::new(std::io::ErrorKind::WriteZero, "Failed To Write Bytes")).into();
                }
                Poll::Ready(Ok(written)) => {
                    tracing::trace!("Wrote {} bytes", written);
                    project.write_buffer.advance(written);
                }
                Poll::Ready(Err(e)) => {
                    tracing::error!("Error writing bytes: {:?}", e);
                    return Err(e).into();
                }
                Poll::Pending => return Poll::Pending,
            }
        }
        project.sock.as_mut().poll_flush(cx)
    }

    #[instrument(skip(self, cx))]
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        tracing::trace!("poll_close called");

        match self.as_mut().poll_flush(cx)? {
            Poll::Ready(()) => self.project().sock.poll_shutdown(cx),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<St> Stream for FramedHandshake<St>
where
    St: AsyncRead + std::fmt::Debug + Unpin,
{
    type Item = Result<HandshakeMessage, std::io::Error>;

    #[instrument(skip(self, cx))]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.state {
            HandshakeState::Waiting => {
                tracing::trace!("handshake waiting...");
                let mut this = self.project();

                assert!(this.read_buffer.is_empty());
                assert_eq!(0, *this.read_pos);

                this.read_buffer.push(0);
                assert_eq!(1, this.read_buffer.len());

                let mut buf = ReadBuf::new(this.read_buffer);
                buf.set_filled(0);

                tracing::trace!("Read Buffer: {buf:?}");
                tracing::trace!("Sock Buffer: {:?}", this.sock);

                {
                    let ready = match this.sock.as_mut().poll_read(cx, &mut buf) {
                        Poll::Ready(ready) => ready,
                        Poll::Pending => {
                            this.read_buffer.clear();
                            tracing::trace!("socket pending...");
                            return Poll::Pending;
                        }
                    };

                    if let Err(e) = ready {
                        tracing::error!("Error reading bytes: {:?}", e);
                        *this.state = HandshakeState::Errored;
                        return Poll::Ready(Some(Err(e)));
                    };
                }

                let filled = buf.filled();

                let byte = match filled.len() {
                    0 => {
                        tracing::trace!("zero bytes read... pending");
                        this.read_buffer.clear();
                        cx.waker().wake_by_ref();
                        return Poll::Pending;
                    }
                    1 => filled[0],
                    2.. => unreachable!("bip_handshake: limited by buffer size {filled:?}"),
                };

                let length = message::write_len_with_protocol_len(byte);

                tracing::debug!("length byte: {byte}, expands to: {length} bytes");

                this.read_buffer.resize(length, 0);
                *this.read_pos = 1;
                *this.state = HandshakeState::Reading;

                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            HandshakeState::Reading => {
                tracing::trace!("handshake reading...");
                let mut this = self.project();

                assert!(!this.read_buffer.is_empty());

                let length = this.read_buffer.len();
                let pos = this.read_pos;

                assert_ne!(0, *pos);
                assert!(*pos < length);

                let mut buf = ReadBuf::new(this.read_buffer);
                buf.set_filled(*pos);
                tracing::trace!("have {pos} bytes out of {length}...");

                {
                    let ready = match this.sock.as_mut().poll_read(cx, &mut buf) {
                        Poll::Ready(ready) => ready,
                        Poll::Pending => {
                            tracing::trace!("socket pending...");
                            return Poll::Pending;
                        }
                    };

                    if let Err(e) = ready {
                        tracing::error!("Error reading bytes: {:?}", e);
                        *this.state = HandshakeState::Errored;
                        return Poll::Ready(Some(Err(e)));
                    };
                }

                let filled = buf.filled().len();
                assert!(filled <= length);
                assert!(*pos <= filled);
                let added = filled - *pos;
                *pos = filled;

                tracing::trace!("read {added} bytes, for a total of: {pos} / {length}...");

                if filled == length {
                    tracing::trace!("have full amount");
                    *this.state = HandshakeState::Ready;
                };

                cx.waker().wake_by_ref();
                return Poll::Pending;
            }

            HandshakeState::Ready => {
                tracing::trace!("handshake ready...");

                assert!(!self.read_buffer.is_empty());
                assert_eq!(self.read_pos, self.read_buffer.len());

                let buf = std::mem::take(&mut self.read_buffer);

                match HandshakeMessage::from_bytes(&buf) {
                    Ok(((), message)) => {
                        tracing::trace!("Parsed HandshakeMessage: {:?}", message);
                        self.state = HandshakeState::Finished;

                        return Poll::Ready(Some(Ok(message)));
                    }
                    Err(nom::Err::Incomplete(needed)) => {
                        tracing::error!("Failed to parse incomplete HandshakeMessage: {needed:?}");
                        self.state = HandshakeState::Errored;

                        return Poll::Ready(Some(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("Failed to parse incomplete HandshakeMessage: {needed:?}"),
                        ))));
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse HandshakeMessage");
                        self.state = HandshakeState::Errored;

                        return Poll::Ready(Some(Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e))));
                    }
                }
            }
            HandshakeState::Finished => {
                tracing::trace!("handshake finished...");
                return Poll::Ready(None);
            }

            HandshakeState::Errored => {
                tracing::warn!("handshake polled while errored...");
                return Poll::Ready(None);
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use std::sync::Once;

    use futures::stream::StreamExt;
    use futures::SinkExt as _;
    use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWriteExt as _};
    use tracing::level_filters::LevelFilter;
    use util::bt::{self, InfoHash, PeerId};

    use super::FramedHandshake;
    use crate::bittorrent::message::HandshakeMessage;
    use crate::message::extensions::{self, Extensions};
    use crate::message::protocol::Protocol;

    pub static INIT: Once = Once::new();

    pub fn tracing_stderr_init(filter: LevelFilter) {
        let builder = tracing_subscriber::fmt()
            .with_max_level(filter)
            .with_ansi(true)
            .with_writer(std::io::stderr);

        builder.pretty().with_file(true).init();

        tracing::info!("Logging initialized");
    }

    fn any_peer_id() -> PeerId {
        [22u8; bt::PEER_ID_LEN].into()
    }

    fn any_info_hash() -> InfoHash {
        [55u8; bt::INFO_HASH_LEN].into()
    }

    fn any_extensions() -> Extensions {
        [255u8; extensions::NUM_EXTENSION_BYTES].into()
    }

    #[tokio::test]
    async fn write_and_read_into_async_buffer() {
        INIT.call_once(|| {
            tracing_stderr_init(LevelFilter::INFO);
        });

        let mut v = vec![0; 100];
        //let mut buf = std::io::Cursor::new(v);
        let mut read_buf = tokio::io::ReadBuf::new(&mut v);

        let data: Box<std::io::Cursor<Vec<u8>>> = Box::new(std::io::Cursor::new((0..100).collect()));

        let data_reader = &mut (data as Box<dyn AsyncRead + Unpin>);

        let mut a = data_reader.read_buf(&mut read_buf).await.unwrap();
        a += data_reader.read_buf(&mut read_buf).await.unwrap();
        a += data_reader.read_buf(&mut read_buf).await.unwrap();

        assert_eq!(a, 100);
    }

    #[tokio::test]
    async fn positive_write_handshake_message() {
        INIT.call_once(|| {
            tracing_stderr_init(LevelFilter::INFO);
        });

        let message = HandshakeMessage::from_parts(Protocol::BitTorrent, any_extensions(), any_info_hash(), any_peer_id());

        let mut framed_handshake = FramedHandshake::new(std::io::Cursor::new(Vec::new()));

        framed_handshake.send(message.clone()).await.unwrap();

        let sock = framed_handshake.into_inner();

        let mut exp_buffer = Vec::new();
        message.write_bytes(&mut exp_buffer).await.unwrap();

        assert_eq!(exp_buffer, sock.into_inner());
    }

    #[tokio::test]
    async fn positive_write_multiple_handshake_messages() {
        INIT.call_once(|| {
            tracing_stderr_init(LevelFilter::INFO);
        });

        let message_one = HandshakeMessage::from_parts(Protocol::BitTorrent, any_extensions(), any_info_hash(), any_peer_id());
        let message_two = HandshakeMessage::from_parts(
            Protocol::Custom(vec![5, 6, 7]),
            any_extensions(),
            any_info_hash(),
            any_peer_id(),
        );

        let mut framed_handshake = FramedHandshake::new(std::io::Cursor::new(Vec::new()));

        framed_handshake.send(message_one.clone()).await.unwrap();
        framed_handshake.send(message_two.clone()).await.unwrap();

        let sock = framed_handshake.into_inner();

        let mut exp_buffer = Vec::new();
        message_one.write_bytes(&mut exp_buffer).await.unwrap();
        message_two.write_bytes(&mut exp_buffer).await.unwrap();

        assert_eq!(exp_buffer, sock.into_inner());
    }

    #[tokio::test]
    async fn positive_read_handshake_message() {
        INIT.call_once(|| {
            tracing_stderr_init(LevelFilter::INFO);
        });

        let exp_message = HandshakeMessage::from_parts(Protocol::BitTorrent, any_extensions(), any_info_hash(), any_peer_id());
        tracing::trace!("Handshake Message: {:?}", exp_message);

        let mut buffer = std::io::Cursor::new(Vec::new());
        exp_message.write_bytes(&mut buffer).await.unwrap();
        buffer.set_position(0);

        tracing::trace!("Buffer before reading: {:?}", buffer);
        let mut framed_handshake = FramedHandshake::new(buffer);

        let recv_message = match framed_handshake.next().await {
            Some(Ok(msg)) => msg,
            Some(Err(e)) => panic!("Error reading message: {e:?}"),
            None => panic!("Expected a message but got None"),
        };
        assert!(framed_handshake.next().await.is_none());

        tracing::trace!("Received message: {:?}", recv_message);

        assert_eq!(exp_message, recv_message);
    }

    #[tokio::test]
    async fn positive_read_byte_after_handshake() {
        INIT.call_once(|| {
            tracing_stderr_init(LevelFilter::INFO);
        });

        let exp_message = HandshakeMessage::from_parts(Protocol::BitTorrent, any_extensions(), any_info_hash(), any_peer_id());

        let mut buffer = std::io::Cursor::new(Vec::new());
        let () = exp_message.write_bytes(&mut buffer).await.unwrap();
        let () = buffer.write_all(&[55]).await.unwrap();
        let () = buffer.set_position(0);

        tracing::trace!("Buffer before reading: {:?}", buffer);

        let mut framed_handshake = FramedHandshake::new(buffer);
        let message = framed_handshake.next().await.unwrap().unwrap();

        assert_eq!(exp_message, message);

        let sock = framed_handshake.into_inner();

        let position: usize = sock.position().try_into().unwrap();
        let buffer = sock.get_ref();
        let remaining = &buffer[position..];

        assert_eq!([55], remaining);
    }

    #[tokio::test]
    async fn positive_read_bytes_after_handshake() {
        INIT.call_once(|| {
            tracing_stderr_init(LevelFilter::INFO);
        });

        let exp_message = HandshakeMessage::from_parts(Protocol::BitTorrent, any_extensions(), any_info_hash(), any_peer_id());

        let mut buffer = Vec::new();
        exp_message.write_bytes(&mut buffer).await.unwrap();
        // Write some bytes right after the handshake, make sure
        // our framed handshake doesn't read/buffer these (we need
        // to be able to read them afterwards)
        drop(buffer.write_all(&[55, 54, 21]).await);

        let read_frame = {
            let mut frame = FramedHandshake::new(&buffer[..]);
            frame.next().await;
            frame
        };

        let buffer_ref = read_frame.into_inner();

        assert_eq!(&[55, 54, 21], buffer_ref);
    }
}
