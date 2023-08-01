# Packages
These packages together are the _BitTorrent Infrastructure Project_:

### [Bencode (bencode)](./bencode/)
A library for parsing and converting bencoded data.

### [Distributed Hash Table (dht)](./dht/)
A library implementing the Bittorrent Mainline Distributed Hash Table.

### [Disk (disk)](./disk/)
A library providing the ```FileSystem``` interface, for managing pieces of files.

### [Handshake (handshake)](./handshake/)
A library providing a trait for custom handshake implementations and it's implementation for the standard bittorrent handshake.

### [Http Tracker (htracker)](./htracker/) (not implemented)
A library assisting communication with bittorrent HTTP trackers.

### [Local Peer Discovery (lpd)](./lpd/) (not implemented)
A library providing a implementation of the bittorrent Local Peer/Service Discovery mechanism.

### [Magnet (magnet)](./magnet/)
A library for parsing and constructing magnet links.

### [Metainfo (metainfo)](./metainfo/)
A library for Parsing and building of bittorrent metainfo files.

### [Peer (peer)](./peer/)
A library assisting communication between with bittorrent peers via the peer wire protocol.

### [Select (select)](./select/)
A library providing the _Bittorrent Infrastructure Project_ piece selection module.

### [Utility (util)](./util/)
A library providing a set of utilities used by the _Bittorrent Infrastructure Project_.

### [uTorrent Transport Protocol (utp)](./utp/) (not implemented)
A library implementing the _uTorrent Transport Protocol_.

### [UDP Tracker (utracker)](./utracker/)
A library for parsing and writing UDP tracker messages.