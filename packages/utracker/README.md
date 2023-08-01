# UDP Tracker (utracker)
This library provides an api for parsing and writing UDP tracker messages.

UDP messages are used for communication to with BitTorrent Trackers.

Trackers provide a centralized solution to peer discovery within the bittorrent eco-system: Clients send messages to a specific set of trackers, updating them with any state changes that have occurred pertaining to the download of files.

This library provides a api with start and stop events, allowing users to use trackers more generically: adding or removing ourselves from a tracker for the purposes of peer discovery for in any application.