# Packages

These packages together are the _Torrust BitTorrent_ packages:

| Crate       | Folder                    | Description                                                    |
| ----------- | ------------------------- | -------------------------------------------------------------- |
| `bencode`   | [bencode](./bencode/)     | Parsing and converting bencoded data                           |
| `dht`       | [dht](./dht/)             | Bittorrent Mainline Distributed Hash Table                     |
| `disk`      | [disk](./disk/)           | `FileSystem` interface for managing torrent pieces on disk     |
| `handshake` | [handshake](./handshake/) | Trait and implementation for the standard BitTorrent handshake |
| `magnet`    | [magnet](./magnet/)       | Parsing and constructing magnet links                          |
| `metainfo`  | [metainfo](./metainfo/)   | Parsing and building bittorrent metainfo (`.torrent`) files    |
| `peer`      | [peer](./peer/)           | Communication with bittorrent peers via the peer wire protocol |
| `select`    | [select](./select/)       | Piece selection algorithm                                      |
| `util`      | [util](./util/)           | Shared utilities used across packages                          |
