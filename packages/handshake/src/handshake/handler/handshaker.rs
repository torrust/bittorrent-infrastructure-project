use std::net::SocketAddr;
use std::time::Duration;

use futures::future::BoxFuture;
use futures::{FutureExt as _, SinkExt as _, StreamExt as _};
use tokio::io::{AsyncRead, AsyncWrite};
use util::bt::PeerId;

use crate::bittorrent::framed::FramedHandshake;
use crate::bittorrent::message::HandshakeMessage;
use crate::filter::filters::Filters;
use crate::handshake::handler;
use crate::handshake::handler::HandshakeType;
use crate::message::complete::CompleteMessage;
use crate::message::extensions::Extensions;
use crate::message::initiate::InitiateMessage;

#[allow(clippy::module_name_repetitions)]
pub fn execute_handshake<'a, S>(
    item: std::io::Result<HandshakeType<S>>,
    context: &(Extensions, PeerId, Filters, Duration),
) -> BoxFuture<'a, std::io::Result<Option<CompleteMessage<S>>>>
where
    S: AsyncWrite + AsyncRead + std::fmt::Debug + Send + Unpin + 'a,
{
    let (ext, pid, filters, timeout) = context;

    match item {
        Ok(HandshakeType::Initiate(sock, init_msg)) => {
            initiate_handshake(sock, init_msg, *ext, *pid, filters.clone(), *timeout).boxed()
        }
        Ok(HandshakeType::Complete(sock, addr)) => complete_handshake(sock, addr, *ext, *pid, filters.clone(), *timeout).boxed(),
        Err(err) => async move { Err(err) }.boxed(),
    }
}

async fn initiate_handshake<S>(
    sock: S,
    init_msg: InitiateMessage,
    ext: Extensions,
    pid: PeerId,
    filters: Filters,
    timeout: Duration,
) -> std::io::Result<Option<CompleteMessage<S>>>
where
    S: AsyncWrite + AsyncRead + std::fmt::Debug + Unpin,
{
    let mut framed = FramedHandshake::new(sock);

    let (prot, hash, addr) = init_msg.into_parts();
    let handshake_msg = HandshakeMessage::from_parts(prot.clone(), ext, hash, pid);

    let send_result = tokio::time::timeout(timeout, framed.send(handshake_msg)).await;
    if send_result.is_err() {
        return Ok(None);
    }

    let recv_result = tokio::time::timeout(timeout, framed.next()).await;
    let Ok(Some(Ok(msg))) = recv_result else { return Ok(None) };

    let (remote_prot, remote_ext, remote_hash, remote_pid) = msg.into_parts();
    let socket = framed.into_inner();

    if remote_hash != hash {
        Err(std::io::Error::new(std::io::ErrorKind::Other, "not matching hash"))
    } else if remote_prot != prot {
        Err(std::io::Error::new(std::io::ErrorKind::Other, "not matching port"))
    } else if handler::should_filter(
        Some(&addr),
        Some(&remote_prot),
        Some(&remote_ext),
        Some(&remote_hash),
        Some(&remote_pid),
        &filters,
    ) {
        Err(std::io::Error::new(std::io::ErrorKind::Other, "should not filter"))
    } else {
        Ok(Some(CompleteMessage::new(
            prot,
            ext.union(&remote_ext),
            hash,
            remote_pid,
            addr,
            socket,
        )))
    }
}

async fn complete_handshake<S>(
    sock: S,
    addr: SocketAddr,
    ext: Extensions,
    pid: PeerId,
    filters: Filters,
    timeout: Duration,
) -> std::io::Result<Option<CompleteMessage<S>>>
where
    S: AsyncWrite + AsyncRead + std::fmt::Debug + Unpin,
{
    let mut framed = FramedHandshake::new(sock);

    let recv_result = tokio::time::timeout(timeout, framed.next()).await;
    let Ok(Some(Ok(msg))) = recv_result else { return Ok(None) };

    let (remote_prot, remote_ext, remote_hash, remote_pid) = msg.into_parts();

    if handler::should_filter(
        Some(&addr),
        Some(&remote_prot),
        Some(&remote_ext),
        Some(&remote_hash),
        Some(&remote_pid),
        &filters,
    ) {
        Err(std::io::Error::new(std::io::ErrorKind::Other, "should not filter"))
    } else {
        let handshake_msg = HandshakeMessage::from_parts(remote_prot.clone(), ext, remote_hash, pid);

        let send_result = tokio::time::timeout(timeout, framed.send(handshake_msg)).await;
        if send_result.is_err() {
            return Ok(None);
        }

        let socket = framed.into_inner();

        Ok(Some(CompleteMessage::new(
            remote_prot,
            ext.union(&remote_ext),
            remote_hash,
            remote_pid,
            addr,
            socket,
        )))
    }
}

#[cfg(test)]
mod tests {

    use std::time::Duration;

    use util::bt::{self, InfoHash, PeerId};

    use super::HandshakeMessage;
    use crate::filter::filters::Filters;
    use crate::handshake::handler::handshaker;
    use crate::message::extensions::{self, Extensions};
    use crate::message::initiate::InitiateMessage;
    use crate::message::protocol::Protocol;

    fn any_peer_id() -> PeerId {
        [22u8; bt::PEER_ID_LEN].into()
    }

    fn any_other_peer_id() -> PeerId {
        [33u8; bt::PEER_ID_LEN].into()
    }

    fn any_info_hash() -> InfoHash {
        [55u8; bt::INFO_HASH_LEN].into()
    }

    fn any_extensions() -> Extensions {
        [255u8; extensions::NUM_EXTENSION_BYTES].into()
    }

    #[tokio::test]
    async fn positive_initiate_handshake() {
        let remote_pid = any_peer_id();
        let remote_addr = "1.2.3.4:5".parse().unwrap();
        let remote_protocol = Protocol::BitTorrent;
        let remote_hash = any_info_hash();
        let remote_message = HandshakeMessage::from_parts(remote_protocol, any_extensions(), remote_hash, remote_pid);

        let mut writer = std::io::Cursor::new(Vec::with_capacity(remote_message.write_len() * 2));
        writer.set_position(remote_message.write_len() as u64);

        remote_message.write_bytes(&mut writer).await.unwrap();
        writer.set_position(0);

        let init_hash = any_info_hash();
        let init_prot = Protocol::BitTorrent;
        let init_message = InitiateMessage::new(init_prot.clone(), init_hash, remote_addr);

        let init_ext = any_extensions();
        let init_pid = any_other_peer_id();
        let init_filters = Filters::new();

        let complete_message = handshaker::initiate_handshake(
            writer,
            init_message,
            init_ext,
            init_pid,
            init_filters,
            Duration::from_millis(100),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(init_prot, *complete_message.protocol());
        assert_eq!(init_ext, *complete_message.extensions());
        assert_eq!(init_hash, *complete_message.hash());
        assert_eq!(remote_pid, *complete_message.peer_id());
        assert_eq!(remote_addr, *complete_message.address());

        let sent_message =
            HandshakeMessage::from_bytes(&complete_message.socket().get_ref()[..remote_message.write_len()].to_vec())
                .unwrap()
                .1;
        let local_message = HandshakeMessage::from_parts(init_prot, init_ext, init_hash, init_pid);

        let recv_message =
            HandshakeMessage::from_bytes(&complete_message.socket().get_ref()[remote_message.write_len()..].to_vec())
                .unwrap()
                .1;

        assert_eq!(local_message, sent_message);
        assert_eq!(remote_message, recv_message);
    }

    #[tokio::test]
    async fn positive_complete_handshake() {
        let remote_pid = any_peer_id();
        let remote_addr = "1.2.3.4:5".parse().unwrap();
        let remote_protocol = Protocol::BitTorrent;
        let remote_hash = any_info_hash();
        let remote_message = HandshakeMessage::from_parts(Protocol::BitTorrent, any_extensions(), remote_hash, remote_pid);

        let mut writer = std::io::Cursor::new(vec![0u8; remote_message.write_len() * 2]);

        remote_message.write_bytes(&mut writer).await.unwrap();
        writer.set_position(0);

        let comp_ext = any_extensions();
        let comp_pid = any_other_peer_id();
        let comp_filters = Filters::new();

        let complete_message = handshaker::complete_handshake(
            writer,
            remote_addr,
            comp_ext,
            comp_pid,
            comp_filters,
            Duration::from_millis(100),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(remote_protocol, *complete_message.protocol());
        assert_eq!(comp_ext, *complete_message.extensions());
        assert_eq!(remote_hash, *complete_message.hash());
        assert_eq!(remote_pid, *complete_message.peer_id());
        assert_eq!(remote_addr, *complete_message.address());

        let sent_message =
            HandshakeMessage::from_bytes(&complete_message.socket().get_ref()[remote_message.write_len()..].to_vec())
                .unwrap()
                .1;
        let local_message = HandshakeMessage::from_parts(remote_protocol, comp_ext, remote_hash, comp_pid);

        let recv_message =
            HandshakeMessage::from_bytes(&complete_message.socket().get_ref()[..remote_message.write_len()].to_vec())
                .unwrap()
                .1;

        assert_eq!(local_message, sent_message);
        assert_eq!(remote_message, recv_message);
    }
}
