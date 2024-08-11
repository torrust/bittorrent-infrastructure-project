use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet, VecDeque};

use bit_set::BitSet;
use bytes::{BufMut, BytesMut};
use futures::task::Task;
use futures::{task, Async, AsyncSink, Poll, Sink, StartSend, Stream};
use handshake::InfoHash;
use metainfo::Metainfo;
use peer::messages::{BitFieldMessage, HaveMessage};
use peer::PeerInfo;

use crate::revelation::error::{RevealError, RevealErrorKind};
use crate::revelation::{IRevealMessage, ORevealMessage};
use crate::ControlMessage;

/// Revelation module that will honestly report any pieces we have to peers.

#[allow(clippy::module_name_repetitions)]
#[derive(Default)]
pub struct HonestRevealModule {
    torrents: HashMap<InfoHash, PeersInfo>,
    out_queue: VecDeque<ORevealMessage>,
    // Shared bytes container to write bitfield messages to
    out_bytes: BytesMut,
    opt_stream: Option<Task>,
}

struct PeersInfo {
    num_pieces: usize,
    status: BitSet<u8>,
    peers: HashSet<PeerInfo>,
}

impl HonestRevealModule {
    /// Create a new `HonestRevelationModule`.
    #[must_use]
    pub fn new() -> HonestRevealModule {
        HonestRevealModule {
            torrents: HashMap::new(),
            out_queue: VecDeque::new(),
            out_bytes: BytesMut::new(),
            opt_stream: None,
        }
    }

    fn add_torrent(&mut self, metainfo: &Metainfo) -> StartSend<IRevealMessage, Box<RevealError>> {
        let info_hash = metainfo.info().info_hash();

        match self.torrents.entry(info_hash) {
            Entry::Occupied(_) => Err(Box::new(RevealError::from_kind(RevealErrorKind::InvalidMetainfoExists {
                hash: info_hash,
            }))),
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

                Ok(AsyncSink::Ready)
            }
        }
    }

    fn remove_torrent(&mut self, metainfo: &Metainfo) -> StartSend<IRevealMessage, Box<RevealError>> {
        let info_hash = metainfo.info().info_hash();

        if self.torrents.remove(&info_hash).is_none() {
            Err(Box::new(RevealError::from_kind(RevealErrorKind::InvalidMetainfoNotExists {
                hash: info_hash,
            })))
        } else {
            Ok(AsyncSink::Ready)
        }
    }

    fn add_peer(&mut self, peer: PeerInfo) -> StartSend<IRevealMessage, Box<RevealError>> {
        let info_hash = *peer.hash();

        let out_bytes = &mut self.out_bytes;
        let out_queue = &mut self.out_queue;
        let Some(peers_info) = self.torrents.get_mut(&info_hash) else {
            return Err(Box::new(RevealError::from_kind(RevealErrorKind::InvalidMetainfoNotExists {
                hash: info_hash,
            })));
        };

        // Add the peer to our list, so we send have messages to them
        peers_info.peers.insert(peer);

        // If our bitfield has any pieces in it, send the bitfield, otherwise, don't send it
        if !peers_info.status.is_empty() {
            // Get our current bitfield, write it to our shared bytes
            let bitfield_slice = peers_info.status.get_ref().storage();
            // Bitfield stores index 0 at bit 7 from the left, we want index 0 to be at bit 0 from the left
            insert_reversed_bits(out_bytes, bitfield_slice);

            // Split off what we wrote, send this in the message, will be re-used on drop
            let bitfield_bytes = out_bytes.split_off(0).freeze();
            let bitfield = BitFieldMessage::new(bitfield_bytes);

            // Enqueue the bitfield message so that we send it to the peer
            out_queue.push_back(ORevealMessage::SendBitField(peer, bitfield));
        }

        Ok(AsyncSink::Ready)
    }

    fn remove_peer(&mut self, peer: PeerInfo) -> StartSend<IRevealMessage, Box<RevealError>> {
        let info_hash = *peer.hash();

        let Some(peers_info) = self.torrents.get_mut(&info_hash) else {
            return Err(Box::new(RevealError::from_kind(RevealErrorKind::InvalidMetainfoNotExists {
                hash: info_hash,
            })));
        };

        peers_info.peers.remove(&peer);

        Ok(AsyncSink::Ready)
    }

    fn insert_piece(&mut self, hash: InfoHash, index: u64) -> StartSend<IRevealMessage, Box<RevealError>> {
        let out_queue = &mut self.out_queue;
        let Some(peers_info) = self.torrents.get_mut(&hash) else {
            return Err(Box::new(RevealError::from_kind(RevealErrorKind::InvalidMetainfoNotExists {
                hash,
            })));
        };

        let index: usize = index.try_into().unwrap();

        if index >= peers_info.num_pieces {
            Err(Box::new(RevealError::from_kind(RevealErrorKind::InvalidPieceOutOfRange {
                index: index.try_into().unwrap(),
                hash,
            })))
        } else {
            // Queue up all have messages
            for peer in &peers_info.peers {
                out_queue.push_back(ORevealMessage::SendHave(*peer, HaveMessage::new(index.try_into().unwrap())));
            }

            // Insert into bitfield
            peers_info.status.insert(index);

            Ok(AsyncSink::Ready)
        }
    }

    //------------------------------------------------------//

    fn check_stream_unblock(&mut self) {
        if !self.out_queue.is_empty() {
            self.opt_stream.take().as_ref().map(Task::notify);
        }
    }
}

/// Inserts the slice into the `BytesMut` but reverses the bits in each byte.
fn insert_reversed_bits(bytes: &mut BytesMut, slice: &[u8]) {
    for mut byte in slice.iter().copied() {
        let mut reversed_byte = 0;

        for _ in 0..8 {
            // Make room for the bit
            reversed_byte <<= 1;
            // Push the last bit over
            reversed_byte |= byte & 0x01;
            // Push the last bit off
            byte >>= 1;
        }

        bytes.put_u8(reversed_byte);
    }
}

impl Sink for HonestRevealModule {
    type SinkItem = IRevealMessage;
    type SinkError = Box<RevealError>;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        let result = match item {
            IRevealMessage::Control(ControlMessage::AddTorrent(metainfo)) => self.add_torrent(&metainfo),
            IRevealMessage::Control(ControlMessage::RemoveTorrent(metainfo)) => self.remove_torrent(&metainfo),
            IRevealMessage::Control(ControlMessage::PeerConnected(info)) => self.add_peer(info),
            IRevealMessage::Control(ControlMessage::PeerDisconnected(info)) => self.remove_peer(info),
            IRevealMessage::FoundGoodPiece(hash, index) => self.insert_piece(hash, index),
            IRevealMessage::Control(ControlMessage::Tick(_))
            | IRevealMessage::ReceivedBitField(_, _)
            | IRevealMessage::ReceivedHave(_, _) => Ok(AsyncSink::Ready),
        };

        self.check_stream_unblock();

        result
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        Ok(Async::Ready(()))
    }
}

impl Stream for HonestRevealModule {
    type Item = ORevealMessage;
    type Error = Box<RevealError>;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        let next_item = self.out_queue.pop_front().map(|item| Ok(Async::Ready(Some(item))));

        next_item.unwrap_or_else(|| {
            self.opt_stream = Some(task::current());

            Ok(Async::NotReady)
        })
    }
}
