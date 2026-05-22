# Select

This library provides the _Torrust BitTorrent_ piece selection algorithm.

Selection is broken up in to three classes of algorithms:

1. **Piece Revelation**: Determining which pieces we should reveal (even if we don't have the piece...) and to whom.
2. **Piece Selection**: Determining what pieces we should download/upload next.
3. **Piece Queueing**: Calculating, given a piece we want to download, which peers should we send such a request to.

We can mix and match different algorithms to create a swarm that may have different characteristics than other swarms.
