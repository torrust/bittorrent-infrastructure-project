---
doc-type: issue
issue-type: bug
status: open
priority: p1
github-issue: 90
spec-path: docs/issues/open/90-export-missing-public-api-in-torrust-info-hash.md
branch: fix/export-missing-public-api-in-torrust-info-hash
last-updated-utc: 2026-06-09 12:00
semantic-links:
  related-artifacts:
    - packages/info-hash/Cargo.toml
    - packages/info-hash/src/lib.rs
    - packages/info-hash/src/info_hash.rs
    - packages/info-hash/HANDOFF-export-missing-public-api.md
  external-links:
    - https://crates.io/crates/torrust-info-hash
    - https://github.com/torrust/torrust-bittorrent/issues/90
---

# Export Missing Public API Items in `torrust-info-hash`

## Summary

The `torrust-info-hash` v0.1.0 crate was published to crates.io but only re-exports `InfoHash` from `lib.rs`. Several other public items that already exist inside `info_hash.rs` — and are used by downstream consumers such as `torrust-tracker` — are not re-exported. This issue covers adding the missing re-exports, cleaning up associated lint suppressions, and publishing `torrust-info-hash v0.2.0`.

## Motivation

The `ConversionError` enum, `INFO_HASH_BYTES_LEN` constant, and `fixture::gen_seeded_infohash` are all public items inside `info_hash.rs` that downstream consumers need. For example, `torrust-tracker` uses `ConversionError` in its HTTP protocol error types. Without re-exporting these items, consumers must either use internal paths or vendor their own copies.

## Background

The initial extraction of the `info_hash` module from `bittorrent-primitives` into this workspace (see issue #87) focused on the `InfoHash` type itself. The `lib.rs` was kept minimal:

```rust
mod info_hash;
pub use self::info_hash::InfoHash;
```

However, the `info_hash.rs` source contains three other `pub` items that downstream code relied on via `bittorrent_primitives::info_hash::*`:

1. **`ConversionError`** — error enum with `NotEnoughBytes` and `TooManyBytes` variants
2. **`INFO_HASH_BYTES_LEN`** — the constant `20`
3. **`fixture::gen_seeded_infohash`** — test helper for generating pseudo-random infohashes

A detailed handoff document was written during extraction but the re-exports were not completed before publishing. See `packages/info-hash/HANDOFF-export-missing-public-api.md` for the full analysis.

### Note on `INFO_HASH_BYTES_LEN` naming

We considered renaming `INFO_HASH_BYTES_LEN` to `INFO_HASH_V1_BYTES_LEN` to avoid ambiguity when BitTorrent v2 (BEP 52) is implemented — v2 uses SHA-256 (32 bytes) for the full infohash, though it's truncated to 20 bytes for the wire protocol and tracker `info_hash` field. However, since v2 support would require many additional changes beyond just this constant (new types, new hash verification, Merkle trees, etc.), we decided to keep the current name and defer any renaming until v2 is actually being implemented. This avoids implying that v1-v2 coexistence logic exists when it does not.

## Scope

### 1. Update `lib.rs` re-exports

Add `ConversionError`, `INFO_HASH_BYTES_LEN`, and `fixture` to the `pub use` statement in `lib.rs`.

### 2. Remove `#[expect(dead_code)]` from `fixture::gen_seeded_infohash`

Once `fixture` is publicly re-exported, the dead_code lint suppression should be removed (or changed to `#[allow(dead_code)]` with a note).

### 3. Bump version to `0.2.0`

Since this adds new public API surface, bump per SemVer.

### 4. Verify the full public API surface

Ensure all four items are accessible via `torrust_info_hash::*`.

## Action Items

| #   | Action                    | Description                                                                                                     |
| --- | ------------------------- | --------------------------------------------------------------------------------------------------------------- |
| 1   | Update `lib.rs`           | Replace single re-export with all public items: `InfoHash`, `ConversionError`, `INFO_HASH_BYTES_LEN`, `fixture` |
| 2   | Clean up lint suppression | Remove `#[expect(dead_code)]` from `fixture::gen_seeded_infohash`                                               |
| 3   | Bump version              | Update `Cargo.toml` version from `0.1.0` to `0.2.0`                                                             |
| 4   | Run workspace validation  | `cargo check`, `cargo test`, `cargo clippy`, `cargo +nightly fmt`, `linter all`                                 |
| 5   | Commit and open PR        | Create branch, commit, push, open PR against `develop`                                                          |
| 6   | Publish to crates.io      | `cargo publish -p torrust-info-hash` after merge                                                                |

## Implementation Plan

| Step | Task                                                             | Status         |
| ---- | ---------------------------------------------------------------- | -------------- |
| 1    | Update `lib.rs` re-exports                                       | ⬜ not-started |
| 2    | Remove `#[expect(dead_code)]` from `fixture`                     | ⬜ not-started |
| 3    | Bump version to `0.2.0` in `Cargo.toml`                          | ⬜ not-started |
| 4    | Run `cargo check --workspace --all-targets --all-features`       | ⬜ not-started |
| 5    | Run `cargo nextest run --workspace --all-targets --all-features` | ⬜ not-started |
| 6    | Run `cargo +nightly fmt --check`                                 | ⬜ not-started |
| 7    | Run `cargo clippy --workspace --all-targets --all-features`      | ⬜ not-started |
| 8    | Run `linter all`                                                 | ⬜ not-started |
| 9    | Create branch, commit, and open PR                               | ⬜ not-started |
| 10   | Merge PR after CI passes                                         | ⬜ not-started |
| 11   | Publish `torrust-info-hash 0.2.0` to crates.io                   | ⬜ not-started |

## Dependencies

| Dependency         | Version | Notes           |
| ------------------ | ------- | --------------- |
| `binascii`         | 0.1     | Already present |
| `serde` (optional) | 1       | Already present |
| `thiserror`        | 2       | Already present |

## Acceptance Criteria

- [ ] `torrust_info_hash::ConversionError` is accessible and usable
- [ ] `torrust_info_hash::INFO_HASH_BYTES_LEN` is accessible and equals `20`
- [ ] `torrust_info_hash::fixture::gen_seeded_infohash(seed)` is accessible and returns `InfoHash`
- [ ] `cargo doc --no-deps` shows all four items in the generated docs
- [ ] All existing tests pass
- [ ] No new clippy warnings
- [ ] Published crate v0.2.0 on crates.io contains all items

## Acceptance Verification

### Automatic checks

| Check      | Command                                                      | Expected    |
| ---------- | ------------------------------------------------------------ | ----------- |
| Build      | `cargo check --workspace --all-targets --all-features`       | Passes      |
| Tests      | `cargo nextest run --workspace --all-targets --all-features` | All pass    |
| Doc tests  | `cargo test --doc --workspace`                               | All pass    |
| Formatting | `cargo +nightly fmt --check`                                 | No changes  |
| Clippy     | `cargo clippy --workspace --all-targets --all-features`      | No warnings |
| Lint       | `linter all`                                                 | All pass    |

### Manual verification

| Scenario          | Steps                                                                     | Expected Result                                                           | Status     | Evidence |
| ----------------- | ------------------------------------------------------------------------- | ------------------------------------------------------------------------- | ---------- | -------- |
| Cargo doc         | `cargo doc --no-deps -p torrust-info-hash --open`                         | Docs show `InfoHash`, `ConversionError`, `INFO_HASH_BYTES_LEN`, `fixture` | ⬜ pending | —        |
| Downstream import | Create a test binary that imports all four items from `torrust_info_hash` | Compiles successfully                                                     | ⬜ pending | —        |

### Post-implementation Acceptance Criteria Review

After all work is complete and the PR is merged, the Acceptance Criteria section above must be re-reviewed against observed behavior before closing this issue.

## Progress Tracking

### Workflow Checkpoints

- [x] **Issue spec drafted** — `docs/issues/drafts/export-missing-public-api-in-torrust-info-hash.md`
- [x] **Issue spec reviewed** — User approved
- [x] **GitHub issue created** — #90
- [x] **Implementation started** — Branch created
- [ ] **Implementation complete** — All action items done
- [x] **PR opened** — #91 against `develop`
- [ ] **PR merged** — Merged to `develop`
- [ ] **Crate published** — `torrust-info-hash v0.2.0` on crates.io
- [ ] **Issue closed** — Acceptance criteria verified

### Progress Log

| Date       | Entry                                                                                   |
| ---------- | --------------------------------------------------------------------------------------- |
| 2026-06-09 | Issue drafted. Missing re-exports identified in `HANDOFF-export-missing-public-api.md`. |
