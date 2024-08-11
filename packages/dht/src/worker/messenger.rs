use std::net::SocketAddr;
use std::sync::Arc;

use futures::channel::mpsc;
use futures::stream::StreamExt;
use futures::SinkExt as _;
use tokio::net::UdpSocket;
use tokio::task;

use crate::worker::OneshotTask;

const OUTGOING_MESSAGE_CAPACITY: usize = 4096;

#[allow(clippy::module_name_repetitions)]
pub fn create_outgoing_messenger(socket: &Arc<UdpSocket>) -> mpsc::Sender<(Vec<u8>, SocketAddr)> {
    #[allow(clippy::type_complexity)]
    let (send, mut recv): (mpsc::Sender<(Vec<u8>, SocketAddr)>, mpsc::Receiver<(Vec<u8>, SocketAddr)>) =
        mpsc::channel(OUTGOING_MESSAGE_CAPACITY);

    let socket = socket.clone();
    task::spawn(async move {
        while let Some((message, addr)) = recv.next().await {
            send_bytes(&socket, &message[..], addr).await;
        }

        tracing::info!("bip_dht: Outgoing messenger received a channel hangup, exiting thread...");
    });

    send
}

async fn send_bytes(socket: &UdpSocket, bytes: &[u8], addr: SocketAddr) {
    let mut bytes_sent = 0;

    while bytes_sent != bytes.len() {
        if let Ok(num_sent) = socket.send_to(&bytes[bytes_sent..], &addr).await {
            bytes_sent += num_sent;
        } else {
            // TODO: Maybe shut down in this case, will fail on every write...
            tracing::warn!(
                "bip_dht: Outgoing messenger failed to write {} bytes to {}; {} bytes written \
                   before error...",
                bytes.len(),
                addr,
                bytes_sent
            );
            break;
        }
    }
}

#[allow(clippy::module_name_repetitions)]
pub fn create_incoming_messenger(socket: Arc<UdpSocket>, send: mpsc::Sender<OneshotTask>) {
    task::spawn(async move {
        let mut buffer = vec![0u8; 1500];

        loop {
            match socket.recv_from(&mut buffer).await {
                Ok((size, addr)) => {
                    let message = buffer[..size].to_vec();
                    if !send_message(&send, message, addr).await {
                        break;
                    }
                }
                Err(_) => {
                    tracing::warn!("bip_dht: Incoming messenger failed to receive bytes...");
                }
            }
        }

        tracing::info!("bip_dht: Incoming messenger received a channel hangup, exiting thread...");
    });
}

async fn send_message(send: &mpsc::Sender<OneshotTask>, bytes: Vec<u8>, addr: SocketAddr) -> bool {
    send.clone().send(OneshotTask::Incoming(bytes, addr)).await.is_ok()
}
