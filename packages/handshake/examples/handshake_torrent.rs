use std::io::BufRead as _;
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;

use futures::SinkExt;
use handshake::transports::TcpTransport;
use handshake::{HandshakerBuilder, InitiateMessage, Protocol};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut stdout = std::io::stdout();
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();

    // Prompt for InfoHash
    prompt(&mut stdout, "Enter An InfoHash In Hex Format: ")?;
    let hex_hash = read_line(&mut lines)?;
    let info_hash = hex_to_bytes(&hex_hash).into();

    // Prompt for Address and Port
    prompt(&mut stdout, "Enter An Address And Port (eg: addr:port): ")?;
    let address = read_line(&mut lines)?;
    let socket_addr = str_to_addr(&address)?;

    // Show up as a uTorrent client...
    let peer_id = (*b"-UT2060-000000000000").into();
    let (mut handshaker, mut tasks) = HandshakerBuilder::new()
        .with_peer_id(peer_id)
        .build(TcpTransport)
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    handshaker
        .send(InitiateMessage::new(Protocol::BitTorrent, info_hash, socket_addr))
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    tracing::trace!("\nConnection With Peer Established...Closing In 10 Seconds");

    sleep(Duration::from_secs(10)).await;

    tasks.shutdown().await;

    Ok(())
}

fn prompt<W: std::io::Write>(writer: &mut W, message: &str) -> std::io::Result<()> {
    writer.write_all(message.as_bytes())?;
    writer.flush()
}

fn read_line<B: std::io::BufRead>(lines: &mut std::io::Lines<B>) -> std::io::Result<String> {
    lines.next().unwrap_or_else(|| Ok(String::new()))
}

fn hex_to_bytes(hex: &str) -> [u8; 20] {
    let mut bytes = [0u8; 20];
    for (i, byte) in bytes.iter_mut().enumerate() {
        let hex_chunk = &hex[i * 2..=i * 2 + 1];
        *byte = u8::from_str_radix(hex_chunk, 16).unwrap();
    }
    bytes
}

fn str_to_addr(addr: &str) -> std::io::Result<SocketAddr> {
    addr.to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid address format"))
}
