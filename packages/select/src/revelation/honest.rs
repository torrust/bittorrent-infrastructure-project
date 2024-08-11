use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet, VecDeque};
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use bit_set::BitSet;
use bytes::{BufMut, BytesMut};
use futures::{Sink, Stream};
use handshake::InfoHash;
use metainfo::Metainfo;
use peer::messages::{BitFieldMessage, HaveMessage};
use peer::PeerInfo;
use tracing::instrument;

use crate::revelation::error::RevealError;
use crate::revelation::{IRevealMessage, ORevealMessage};
use crate::ControlMessage;

#[allow(clippy::module_name_repetitions)]
#[derive(Default)]
pub struct HonestRevealModuleBuilder {
    torrents: HashMap<InfoHash, PeersInfo>,
    out_queue: VecDeque<ORevealMessage>,
    out_bytes: BytesMut,
}

impl HonestRevealModuleBuilder {
    #[must_use]
    pub fn new() -> HonestRevealModuleBuilder {
        HonestRevealModuleBuilder {
            torrents: HashMap::new(),
            out_queue: VecDeque::new(),
            out_bytes: BytesMut::new(),
        }
    }

    #[must_use]
    pub fn build(self) -> HonestRevealModule {
        HonestRevealModule::from_builder(self)
    }
}

struct PeersInfo {
    num_pieces: usize,
    status: BitSet<u8>,
    peers: HashSet<PeerInfo>,
}

#[allow(clippy::module_name_repetitions)]
pub struct HonestRevealModule {
    torrents: HashMap<InfoHash, PeersInfo>,
    out_queue: VecDeque<ORevealMessage>,
    out_bytes: BytesMut,
    opt_stream_waker: Option<Waker>,
}

impl HonestRevealModule {
    #[must_use]
    pub fn from_builder(builder: HonestRevealModuleBuilder) -> HonestRevealModule {
        HonestRevealModule {
            torrents: builder.torrents,
            out_queue: builder.out_queue,
            out_bytes: builder.out_bytes,
            opt_stream_waker: None,
        }
    }

    fn handle_message(&mut self, message: IRevealMessage) -> Result<(), RevealError> {
        match message {
            IRevealMessage::Control(ControlMessage::AddTorrent(metainfo)) => self.add_torrent(&metainfo),
            IRevealMessage::Control(ControlMessage::RemoveTorrent(metainfo)) => self.remove_torrent(&metainfo),
            IRevealMessage::Control(ControlMessage::PeerConnected(info)) => self.add_peer(info),
            IRevealMessage::Control(ControlMessage::PeerDisconnected(info)) => self.remove_peer(info),
            IRevealMessage::FoundGoodPiece(hash, index) => self.insert_piece(hash, index),
            IRevealMessage::Control(ControlMessage::Tick(_))
            | IRevealMessage::ReceivedBitField(_, _)
            | IRevealMessage::ReceivedHave(_, _) => Ok(()),
        }
    }

    #[instrument(skip(self))]
    fn add_torrent(&mut self, metainfo: &Metainfo) -> Result<(), RevealError> {
        tracing::trace!("adding torrent");

        let info_hash = metainfo.info().info_hash();

        match self.torrents.entry(info_hash) {
            Entry::Occupied(_) => {
                tracing::error!("invalid metainfo");
                Err(RevealError::InvalidMetainfoExists { hash: info_hash })
            }
            Entry::Vacant(vac) => {
                let num_pieces = metainfo.info().pieces().count();

                let mut piece_set = BitSet::default();
                piece_set.reserve_len_exact(num_pieces);

                let peers_info = PeersInfo {
                    num_pieces,
                    status: piece_set,
                    peers: HashSet::new(),
                };
                vac.insert(peers_info);

                Ok(())
            }
        }
    }

    #[instrument(skip(self))]
    fn remove_torrent(&mut self, metainfo: &Metainfo) -> Result<(), RevealError> {
        tracing::trace!("removing torrent");

        let info_hash = metainfo.info().info_hash();

        if self.torrents.remove(&info_hash).is_none() {
            Err(RevealError::InvalidMetainfoNotExists { hash: info_hash })
        } else {
            Ok(())
        }
    }

    #[instrument(skip(self))]
    fn add_peer(&mut self, peer: PeerInfo) -> Result<(), RevealError> {
        tracing::trace!("adding peer");
        let info_hash = *peer.hash();

        let out_bytes = &mut self.out_bytes;
        let out_queue = &mut self.out_queue;
        let Some(peers_info) = self.torrents.get_mut(&info_hash) else {
            tracing::error!("adding peer error");
            return Err(RevealError::InvalidMetainfoNotExists { hash: info_hash });
        };

        peers_info.peers.insert(peer);

        if !peers_info.status.is_empty() {
            let bitfield_slice = peers_info.status.get_ref().storage();
            insert_reversed_bits(out_bytes, bitfield_slice);

            let bitfield_bytes = out_bytes.split_off(0).freeze();
            let bitfield = BitFieldMessage::new(bitfield_bytes);

            let message = ORevealMessage::SendBitField(peer, bitfield);
            tracing::trace!("sending message: {message:?}");

            out_queue.push_back(message);
            if let Some(waker) = self.opt_stream_waker.take() {
                waker.wake();
            }
        }

        Ok(())
    }

    #[instrument(skip(self))]
    fn remove_peer(&mut self, peer: PeerInfo) -> Result<(), RevealError> {
        let info_hash = *peer.hash();

        let Some(peers_info) = self.torrents.get_mut(&info_hash) else {
            return Err(RevealError::InvalidMetainfoNotExists { hash: info_hash });
        };

        peers_info.peers.remove(&peer);

        Ok(())
    }

    #[instrument(skip(self))]
    fn insert_piece(&mut self, hash: InfoHash, index: u64) -> Result<(), RevealError> {
        tracing::trace!("inserting piece");

        let out_queue = &mut self.out_queue;
        let Some(peers_info) = self.torrents.get_mut(&hash) else {
            return Err(RevealError::InvalidMetainfoNotExists { hash });
        };

        let index: usize = index.try_into().unwrap();

        if index >= peers_info.num_pieces {
            Err(RevealError::InvalidPieceOutOfRange {
                index: index.try_into().unwrap(),
                hash,
            })
        } else {
            for peer in &peers_info.peers {
                let message = ORevealMessage::SendHave(*peer, HaveMessage::new(index.try_into().unwrap()));
                tracing::trace!("sending message: {message:?}");

                out_queue.push_back(message);

                if let Some(waker) = self.opt_stream_waker.take() {
                    waker.wake();
                }
            }

            peers_info.status.insert(index);

            Ok(())
        }
    }

    #[instrument(skip(self))]
    fn poll_next_message(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<ORevealMessage, RevealError>>> {
        tracing::trace!("polling for next message");

        if let Some(message) = self.out_queue.pop_front() {
            tracing::trace!("sending message {message:?}");

            Poll::Ready(Some(Ok(message)))
        } else {
            tracing::trace!("no messages found... pending");

            self.opt_stream_waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl Sink<IRevealMessage> for HonestRevealModule {
    type Error = RevealError;

    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: IRevealMessage) -> Result<(), Self::Error> {
        self.handle_message(item)
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_flush(cx)
    }
}

impl Stream for HonestRevealModule {
    type Item = Result<ORevealMessage, RevealError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.poll_next_message(cx)
    }
}

fn insert_reversed_bits(bytes: &mut BytesMut, slice: &[u8]) {
    for mut byte in slice.iter().copied() {
        let mut reversed_byte = 0;

        for _ in 0..8 {
            reversed_byte <<= 1;
            reversed_byte |= byte & 0x01;
            byte >>= 1;
        }

        bytes.put_u8(reversed_byte);
    }
}
