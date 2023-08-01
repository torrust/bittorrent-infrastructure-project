# Magnet
This library provides an api for parsing and building of bittorrent metainfo files.


Metainfo files serve the purpose of providing a list of checksums for clients interested in specific files, how long each hashed piece should be, and the directory structure for the files.

This allows clients to verify the integrity of received files, as well as the ability to recreate exactly the directory structure for the files.

Aside from that, there is a plethora of optional information that can be included in this file such as nodes to be contacted in the DHT, trackers to contact, as well as comments, date created, who created the metainfo file, etc.