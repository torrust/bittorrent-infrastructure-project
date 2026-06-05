# torrust-bencode

[![Crates.io](https://img.shields.io/crates/v/torrust-bencode.svg)](https://crates.io/crates/torrust-bencode)

This library allows for the creation and parsing of bencode encodings.

Bencode is the binary encoding used throughout bittorrent technologies from metainfo files to DHT messages. Bencode types include integers, byte arrays, lists, and dictionaries, of which the last two can hold any bencode type (they could be recursively constructed).

## Package History

This crate has moved through several repositories and crate names:

| Period   | Repository                                                                                       | Crate name on crates.io                                                                                                      |
| -------- | ------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------- |
| Original | [GGist/bip-rs](https://github.com/GGist/bip-rs)                                                  | `bip-bencode` (unpublished / abandoned)                                                                                      |
| Interim  | [torrust/torrust-tracker](https://github.com/torrust/torrust-tracker) (`contrib/bencode`)        | [`torrust-tracker-contrib-bencode`](https://crates.io/crates/torrust-tracker-contrib-bencode) — last published as **v3.0.0** |
| Current  | [torrust/torrust-bittorrent](https://github.com/torrust/torrust-bittorrent) (`packages/bencode`) | [`torrust-bencode`](https://crates.io/crates/torrust-bencode) — from **v3.0.0** onwards                                      |

### Why did the crate name change?

When the Torrust project consolidated its BitTorrent infrastructure libraries into the dedicated
`torrust-bittorrent` workspace, the `bencode` package was migrated here from `torrust-tracker`
where it had been maintained as a contrib package. The new crate name `torrust-bencode` follows
the organisation-wide naming convention (`torrust-<short-name>`) established in the
[publishing and versioning ADR](../../docs/adrs/20260605103740_crate_publishing_and_versioning.md).

The version number was preserved at `3.0.0` to avoid confusion for consumers already using
`torrust-tracker-contrib-bencode v3.0.0` — the code is identical.
