# Select
This library provides the _Bittorrent Infrastructure Project_ piece selection algorithm.

Selection is broken up in to three classes of algorithms:
1. __Piece Revelation__: Determining which pieces we should reveal (even if we don't have the piece...) and to whom.
2. __Piece Selection__: Determining what pieces we should download/upload next.
3. __Piece Queueing__: Calculating, given a piece we want to download, which peers should we send such a request to.

We can mix and match different algorithms to create a swarm that may have different characteristics than other swarms.