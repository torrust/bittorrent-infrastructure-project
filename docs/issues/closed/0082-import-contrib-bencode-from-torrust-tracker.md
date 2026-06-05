---
doc-type: issue
issue-type: task
status: closed
priority: p1
github-issue: 82
spec-path: docs/issues/closed/0082-import-contrib-bencode-from-torrust-tracker.md
branch: 82-import-contrib-bencode-from-torrust-tracker
related-pr: 84
last-updated-utc: 2026-06-05 15:30
semantic-links:
  skill-links:
    - release-package
  related-artifacts:
    - packages/bencode/Cargo.toml
    - packages/bencode/src/
    - docs/adrs/20260605103740_crate_publishing_and_versioning.md
  external-links:
    - https://github.com/torrust/torrust-tracker/issues/1881
    - https://crates.io/crates/torrust-tracker-contrib-bencode
---

# Import `torrust-tracker-contrib-bencode` from Torrust Tracker

## Summary

Import the updated `bencode` package from [torrust/torrust-tracker](https://github.com/torrust/torrust-tracker)
into this workspace as `torrust-bencode`, preserving the version `3.0.0` that was previously published to
crates.io under the name `torrust-tracker-contrib-bencode`.

## Motivation

The `bencode` package in this workspace was originally forked from the unmaintained
[GGist/bip-rs](https://github.com/GGist/bip-rs). In parallel, Cameron ([@da2ce7](https://github.com/da2ce7))
had already updated and published a refined version of the same package from within the
`torrust/torrust-tracker` workspace as `torrust-tracker-contrib-bencode v3.0.0`.

When this repository was set up as the canonical home for BitTorrent infrastructure crates, those
tracker-side improvements had not yet been back-ported. This task closes that gap by importing the
changes from `torrust/torrust-tracker` into `packages/bencode` and publishing the result as the
new `torrust-bencode` crate on crates.io.

## Background

- The corresponding tracker-side issue is [torrust/torrust-tracker#1881](https://github.com/torrust/torrust-tracker/issues/1881).
- The previously published crate: [crates.io/crates/torrust-tracker-contrib-bencode v3.0.0](https://crates.io/crates/torrust-tracker-contrib-bencode).
- The ADR [docs/adrs/20260605103740_crate_publishing_and_versioning.md](../../adrs/20260605103740_crate_publishing_and_versioning.md)
  was updated to include an exception for this case: imported crates that were already published
  under a different name retain their prior version number rather than resetting to `0.1.0`.

## Scope

### 1. Import source changes from `torrust/torrust-tracker`

Apply the following changes from `torrust-tracker/contrib/bencode` to `packages/bencode`:

| File                      | Change                                                     |
| ------------------------- | ---------------------------------------------------------- |
| `Cargo.toml`              | Set `publish = true`, `version = "3.0.0"`, update keywords |
| `src/access/convert.rs`   | Refactor `lookup` call to use `map_or_else` (clippy fix)   |
| `src/lib.rs`              | Add `extern crate` and `#[macro_use]` to doc examples      |
| `src/reference/decode.rs` | Add blank line for readability                             |

### 2. Update ADR versioning policy

Amend `docs/adrs/20260605103740_crate_publishing_and_versioning.md` to document the exception:
when a crate is imported from another Torrust repository where it was already published under a
different name, the prior version is preserved to maintain continuity for existing consumers.

### 3. Publish `torrust-bencode` to crates.io

Perform a dry-run first (`cargo publish -p torrust-bencode --dry-run`), then publish the crate
as `torrust-bencode v3.0.0`.

## Acceptance Criteria

- [ ] `packages/bencode/Cargo.toml` has `publish = true` and `version = "3.0.0"`.
- [ ] All source changes from `torrust-tracker-contrib-bencode v3.0.0` are applied to `packages/bencode/src/`.
- [ ] `cargo check --workspace --all-targets --all-features` passes.
- [ ] `cargo nextest run -p torrust-bencode` passes.
- [ ] `cargo publish -p torrust-bencode --dry-run` succeeds.
- [ ] `linter all` passes.
- [ ] ADR updated with the imported-crate version exception policy.
- [ ] GitHub issue opened with a link to [torrust/torrust-tracker#1881](https://github.com/torrust/torrust-tracker/issues/1881).

## Out of Scope

- Importing other packages from `torrust/torrust-tracker` (separate issues).
- Any API changes beyond what was already in `torrust-tracker-contrib-bencode v3.0.0`.

## Related

- [torrust/torrust-tracker#1881](https://github.com/torrust/torrust-tracker/issues/1881) — corresponding tracker-side issue.
- [crates.io/crates/torrust-tracker-contrib-bencode](https://crates.io/crates/torrust-tracker-contrib-bencode) — prior published crate.
- [#80](https://github.com/torrust/torrust-bittorrent/issues/80) — package publishing infrastructure (prerequisite).
