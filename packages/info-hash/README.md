# torrust-info-hash

In-house crate for BitTorrent `InfoHash` v1 type.

## Origin and In-House Maintenance

This crate was originally extracted from the standalone
[torrust/bittorrent-primitives](https://github.com/torrust/bittorrent-primitives)
repository and moved into the [torrust/torrust-bittorrent](https://github.com/torrust/torrust-bittorrent)
workspace.

Torrust keeps this package in-house to ensure ongoing maintenance, dependency
updates, and evolution alongside other BitTorrent primitives.

## Licensing and Notices

The original source is dual-licensed under MIT or Apache-2.0. This in-house
package is relicensed under Apache-2.0 to match the workspace license.

An explicit copy of Apache-2.0 is included at [LICENSE-APACHE](./LICENSE-APACHE).

## Usage

```rust
use torrust_info_hash::InfoHash;

let info_hash: InfoHash = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF"
    .parse()
    .expect("valid 40-char hex string");

assert_eq!(info_hash.to_hex_string(), "ffffffffffffffffffffffffffffffffffffffff");
```

## Features

| Feature | Default | Description                      |
| ------- | ------- | -------------------------------- |
| `serde` | Yes     | Enable serde serialization/deserialization |
