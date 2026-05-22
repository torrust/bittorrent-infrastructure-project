# Torrust BitTorrent　[![coverage_wf_b]][coverage_wf] [![testing_wf_b]][testing_wf]

<div align="center"><img src="./docs/media/torrust-bittorrent-logo.svg" alt="Torrust BitTorrent Logo" width="200" align="center"/></div>

**_A collection of [packages][rel_packages] that can support the building of applications using [BitTorrent] technologies._**

This repository is a fork of [GGist]'s project: [bip-rs].

In this fork we have:

- [x] Reorganized the crate collection into a cargo workspace packages. ([#6])
- [x] Switched to relative dependencies between packages. ([#6])
- [x] Modernized the code to compile with the current version of rust. ([#6], [#7])
- [x] Implemented continuous integration using github workflows. ([#8])
- [x] Update some of the project dependencies. ([#9], [#17], [#26], [#27])
- [x] Preformed a general cleanup of the codebase. ([#10], [#16], [#18], [#29], [#31])
- [x] Updated all dependencies to modern versions. ( [#19], [#20], [#21], [#22], [#23], [#25])
- [x] Removed the `utracker` package (superseded by [torrust-tracker] for UDP tracker support). ([#53], [#70])

The future goals are:

- [ ] Publish updated versions of the crates. ([#37])
- [ ] Increase coverage of unit tests. ([#38])
- [ ] Overhaul the old `mio` architecture in the `dht` package. Making better use of Tokio. ([#54])

**We would like to make a special thanks to all the developers who had contributed to and created this great project.**

---

### Crates

> We have not published any crates from this repository yet.
> Before publishing, we need to complete the ongoing refactoring work and settle on
> a crate naming convention (see [#64]). The leading candidates for the prefix are
> `torrust-` (e.g. `torrust-bencode`) and `torrust-bittorrent-` (e.g. `torrust-bittorrent-bencode`).
> See [#37] for the publishing milestone.

The current (working) crate names, subject to renaming before publication:

| Crate       | Description                                                    |
| ----------- | -------------------------------------------------------------- |
| `bencode`   | Parsing and converting bencoded data                           |
| `dht`       | Bittorrent Mainline Distributed Hash Table                     |
| `disk`      | `FileSystem` interface for managing torrent pieces on disk     |
| `handshake` | Trait and implementation for the standard BitTorrent handshake |
| `magnet`    | Parsing and constructing magnet links                          |
| `metainfo`  | Parsing and building bittorrent metainfo (`.torrent`) files    |
| `peer`      | Communication with bittorrent peers via the peer wire protocol |
| `select`    | Piece selection algorithm                                      |
| `util`      | Shared utilities used across packages                          |

#### Original crates published by [GGist]

> **Note:** These crates have not been updated since mid-2018.

The original crates were published under the `bip_` prefix, short for _BitTorrent Infrastructure Project_ (the former name of this project).

[![b_bip_bencode]][c_bip_bencode]　
[![b_bip_disk]][c_bip_disk]　
[![b_bip_handshake]][c_bip_handshake]　
[![b_bip_peer]][c_bip_peer]　
[![b_bip_select]][c_bip_select]　
[![b_bip_dht]][c_bip_dht]　
[![b_bip_metainfo]][c_bip_metainfo]　
[![b_bip_utracker]][c_bip_utracker]　

## License

> **_Please note:_** The license of this repository has been changed!
>
> > `Apache-2.0`　and/or　`MIT`　→　*(only)*　`Apache-2.0`
>
> If this is a particular issue for your project, please open an issue.</br>
> _The primary motivation that it has [some software-patent protections][apache-2-patent-license]._

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the [Apache-2.0 License][rel_copyright], shall licensed as above, without any
additional terms or conditions.

[coverage_wf]: ../../actions/workflows/coverage.yaml
[coverage_wf_b]: ../../actions/workflows/coverage.yaml/badge.svg
[testing_wf]: ../../actions/workflows/testing.yaml
[testing_wf_b]: ../../actions/workflows/testing.yaml/badge.svg
[rel_packages]: ./packages/README.md
[rel_copyright]: ./COPYRIGHT
[BitTorrent]: https://www.bittorrent.org/introduction.html
[GGist]: https://github.com/GGist/
[bip-rs]: https://github.com/GGist/bip-rs
[apache-2-patent-license]: https://opensource.com/article/18/2/apache-2-patent-license
[#6]: https://github.com/torrust/torrust-bittorrent/pull/6
[#7]: https://github.com/torrust/torrust-bittorrent/pull/7
[#8]: https://github.com/torrust/torrust-bittorrent/pull/8
[#9]: https://github.com/torrust/torrust-bittorrent/pull/9
[#10]: https://github.com/torrust/torrust-bittorrent/pull/10
[#16]: https://github.com/torrust/torrust-bittorrent/pull/16
[#17]: https://github.com/torrust/torrust-bittorrent/pull/17
[#18]: https://github.com/torrust/torrust-bittorrent/pull/18
[#26]: https://github.com/torrust/torrust-bittorrent/pull/26
[#27]: https://github.com/torrust/torrust-bittorrent/pull/27
[#29]: https://github.com/torrust/torrust-bittorrent/pull/29
[#31]: https://github.com/torrust/torrust-bittorrent/pull/31
[#19]: https://github.com/torrust/torrust-bittorrent/issues/19
[#20]: https://github.com/torrust/torrust-bittorrent/issues/20
[#21]: https://github.com/torrust/torrust-bittorrent/issues/21
[#22]: https://github.com/torrust/torrust-bittorrent/issues/22
[#23]: https://github.com/torrust/torrust-bittorrent/issues/23
[#25]: https://github.com/torrust/torrust-bittorrent/issues/25
[#37]: https://github.com/torrust/torrust-bittorrent/issues/37
[#38]: https://github.com/torrust/torrust-bittorrent/issues/38
[#53]: https://github.com/torrust/torrust-bittorrent/issues/53
[#54]: https://github.com/torrust/torrust-bittorrent/issues/54
[#64]: https://github.com/torrust/torrust-bittorrent/issues/64
[#70]: https://github.com/torrust/torrust-bittorrent/pull/70
[torrust-tracker]: https://github.com/torrust/torrust-tracker
[t_i37]: https://img.shields.io/github/issues/detail/title/torrust/torrust-bittorrent/37?style=for-the-badge&
[s_i37]: https://img.shields.io/github/issues/detail/state/torrust/torrust-bittorrent/37?style=for-the-badge&label=%E3%80%80
[b_bip_bencode]: https://img.shields.io/crates/v/bip_bencode?style=for-the-badge&label=bip_bencode
[b_bip_disk]: https://img.shields.io/crates/v/bip_disk?style=for-the-badge&label=bip_disk
[b_bip_handshake]: https://img.shields.io/crates/v/bip_handshake?style=for-the-badge&label=bip_handshake
[b_bip_peer]: https://img.shields.io/crates/v/bip_peer?style=for-the-badge&label=bip_peer
[b_bip_select]: https://img.shields.io/crates/v/bip_select?style=for-the-badge&label=bip_select
[b_bip_dht]: https://img.shields.io/crates/v/bip_dht?style=for-the-badge&label=bip_dht
[b_bip_metainfo]: https://img.shields.io/crates/v/bip_metainfo?style=for-the-badge&label=bip_metainfo
[b_bip_utracker]: https://img.shields.io/crates/v/bip_utracker?style=for-the-badge&label=bip_utracker
[c_bip_bencode]: https://crates.io/crates/bip_bencode
[c_bip_disk]: https://crates.io/crates/bip_disk
[c_bip_handshake]: https://crates.io/crates/bip_handshake
[c_bip_peer]: https://crates.io/crates/bip_peer
[c_bip_select]: https://crates.io/crates/bip_select
[c_bip_dht]: https://crates.io/crates/bip_dht
[c_bip_metainfo]: https://crates.io/crates/bip_metainfo
[c_bip_utracker]: https://crates.io/crates/bip_utracker
