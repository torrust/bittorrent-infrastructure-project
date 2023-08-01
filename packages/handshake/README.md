# Handshake
This library provides an implementation for handshaking between peers.

Handshaking is the process of connecting to a peer and exchanging information related to how a peer will be communicating with you and vice versa.

In our case, there are many bittorrent technologies that could generally be considered peer discovery mechanisms (local peer discovery, dht, trackers, peer exchange) where once a peer is discovered, a client may want to immediately attempt to establish a connection via a handshake.

This module provides a trait for custom handshake implementations, as well as the standard bittorrent handshake, so that clients can specify a handshaking mechanism for peer discovery services to forward contact information along to.