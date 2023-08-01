# Disk
This disk management library is all about storing/loading pieces to/from any object implementing the ```FileSystem``` interface.

Allowing torrent storage could to be transparently sent to disk, stored in memory, pushed to a distributed file system, or even uploaded to the cloud as pieces come in.

In addition, notifications are sent when good or bad pieces are detected as soon as enough blocks are sent to the disk manager that make up a full piece.