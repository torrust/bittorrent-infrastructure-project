# BitTorrent Infrastructure Project　[![coverage_wf_b]][coverage_wf] [![testing_wf_b]][testing_wf]
<div align="center"><img src="./docs/media/bittorrent-infrastructure-project-logo.svg" alt="Bittorrent Infrastructure Project Logo" width="200" align="center"/></div>

___A collection of [packages][rel_packages] that can support the building of applications using [BitTorrent] technologies.___

This repository is a fork of [GGist]'s project: [bip-rs].

In this fork we have:

- [x] Reorganized the crate collection into a cargo workspace packages. ([#6])
- [x] Switched to relative dependencies between packages. ([#6])
- [x] Modernized the code to compile with the current version of rust. ([#6], [#7])
- [x] Implemented continuous integration using github workflows. ([#8])
- [x] Update some of the project dependencies. ([#9], [#17], [#26], [#27])
- [x] Preformed a general cleanup of the codebase. ([#10], [#16], [#18], [#29], [#31])

The future goals are:

- [ ] Update the other dependencies (__Significant Work Required__). ( [#19], [#20], [#21], [#22], [#23], [#25])
- [ ] Publish updated versions of the crates. ([#37])
- [ ] Increase coverage of unit tests. ([#38])

__We would like to make a special thanks to all the developers who had contributed to and created this great project.__

---

### Crates
[![t_i37]![s_i37]][#37]

> We have not published any crates from this repository yet.</br>
Please see Issue: [#37], to track the progress towards publishing updated crates.

#### Here are the links to the original crates published by [GGist]:

> __Note:__ These crates have not been updated since mid-2018.

[![b_bip_bencode]][c_bip_bencode]　
[![b_bip_disk]][c_bip_disk]　
[![b_bip_handshake]][c_bip_handshake]　
[![b_bip_peer]][c_bip_peer]　
[![b_bip_select]][c_bip_select]　
[![b_bip_dht]][c_bip_dht]　
[![b_bip_metainfo]][c_bip_metainfo]　
[![b_bip_utracker]][c_bip_utracker]　


## License

>___Please note:___ The license of this repository has been changed!
>> `Apache-2.0`　and/or　`MIT`　→　_(only)_　`Apache-2.0`
>
>If this is a particular issue for your project, please open an issue.</br>
>_The primary motivation that it has [some software-patent protections][apache-2-patent-license]._

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
[GGist]:  https://github.com/GGist/
[bip-rs]: https://github.com/GGist/bip-rs
[apache-2-patent-license]: https://opensource.com/article/18/2/apache-2-patent-license



[#6]:  https://github.com/torrust/bittorrent-infrastructure-project/pull/6
[#7]:  https://github.com/torrust/bittorrent-infrastructure-project/pull/7
[#8]:  https://github.com/torrust/bittorrent-infrastructure-project/pull/8
[#9]:  https://github.com/torrust/bittorrent-infrastructure-project/pull/9
[#10]: https://github.com/torrust/bittorrent-infrastructure-project/pull/10
[#16]: https://github.com/torrust/bittorrent-infrastructure-project/pull/16
[#17]: https://github.com/torrust/bittorrent-infrastructure-project/pull/17
[#18]: https://github.com/torrust/bittorrent-infrastructure-project/pull/18
[#26]: https://github.com/torrust/bittorrent-infrastructure-project/pull/26
[#27]: https://github.com/torrust/bittorrent-infrastructure-project/pull/27
[#29]: https://github.com/torrust/bittorrent-infrastructure-project/pull/29
[#31]: https://github.com/torrust/bittorrent-infrastructure-project/pull/31

[#19]: https://github.com/torrust/bittorrent-infrastructure-project/issues/19
[#20]: https://github.com/torrust/bittorrent-infrastructure-project/issues/20
[#21]: https://github.com/torrust/bittorrent-infrastructure-project/issues/21
[#22]: https://github.com/torrust/bittorrent-infrastructure-project/issues/22
[#23]: https://github.com/torrust/bittorrent-infrastructure-project/issues/23
[#25]: https://github.com/torrust/bittorrent-infrastructure-project/issues/25
[#37]: https://github.com/torrust/bittorrent-infrastructure-project/issues/37
[#38]: https://github.com/torrust/bittorrent-infrastructure-project/issues/38

[t_i37]: https://img.shields.io/github/issues/detail/title/torrust/bittorrent-infrastructure-project/37?style=for-the-badge&
[s_i37]: https://img.shields.io/github/issues/detail/state/torrust/bittorrent-infrastructure-project/37?style=for-the-badge&label=%E3%80%80

[b_bip_bencode]:    https://img.shields.io/crates/v/bip_bencode?style=for-the-badge&label=bip_bencode
[b_bip_disk]:       https://img.shields.io/crates/v/bip_disk?style=for-the-badge&label=bip_disk
[b_bip_handshake]:  https://img.shields.io/crates/v/bip_handshake?style=for-the-badge&label=bip_handshake
[b_bip_peer]:       https://img.shields.io/crates/v/bip_peer?style=for-the-badge&label=bip_peer
[b_bip_select]:     https://img.shields.io/crates/v/bip_select?style=for-the-badge&label=bip_select
[b_bip_dht]:        https://img.shields.io/crates/v/bip_dht?style=for-the-badge&label=bip_dht
[b_bip_metainfo]:   https://img.shields.io/crates/v/bip_metainfo?style=for-the-badge&label=bip_metainfo
[b_bip_utracker]:   https://img.shields.io/crates/v/bip_utracker?style=for-the-badge&label=bip_utracker
[c_bip_bencode]:    https://crates.io/crates/bip_bencode
[c_bip_disk]:       https://crates.io/crates/bip_disk
[c_bip_handshake]:  https://crates.io/crates/bip_handshake
[c_bip_peer]:       https://crates.io/crates/bip_peer
[c_bip_select]:     https://crates.io/crates/bip_select
[c_bip_dht]:        https://crates.io/crates/bip_dht
[c_bip_metainfo]:   https://crates.io/crates/bip_metainfo
[c_bip_utracker]:   https://crates.io/crates/bip_utracker