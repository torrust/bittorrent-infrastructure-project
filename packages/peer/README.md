# Peer
This library assists communication between with bittorrent peers via the peer wire protocol.

Peer communication with bittorrent involves:
- Choking (telling someone we won't respond to them now).
- Expressing interest (telling someone, if we were unchoked, we would be interested in some data they hold).

As well as downloading and uploading the actual blocks to peers.

This package defines some common bittorrent messages, including those as part of the `ExtensionBits` in `bip_handshake`, as well as those included in the [extension protocol (BEP-10)](http://www.bittorrent.org/beps/bep_0010.html).

We also provide a `PeerManager` for heartbeating peers and multiplexing messages sent to/from peers so that clients have an easier time communicating asynchronously with many peers.