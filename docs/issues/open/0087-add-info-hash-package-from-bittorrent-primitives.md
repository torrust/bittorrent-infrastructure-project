---
doc-type: issue
issue-type: task
status: open
priority: p1
github-issue: 87
spec-path: docs/issues/open/0087-add-info-hash-package-from-bittorrent-primitives.md
branch: feat/add-info-hash-from-bittorrent-primitives
last-updated-utc: 2026-06-08 12:00
semantic-links:
  skill-links:
    - release-package
  related-artifacts:
    - packages/info-hash/Cargo.toml
    - packages/info-hash/src/
    - Cargo.toml
  external-links:
    - https://github.com/torrust/bittorrent-primitives
    - https://github.com/torrust/bittorrent-primitives/issues/5
    - https://github.com/torrust/torrust-bittorrent/issues/64
---

# Add `torrust-info-hash` Package from `bittorrent-primitives`

## Summary

Import the `info_hash` module from [torrust/bittorrent-primitives](https://github.com/torrust/bittorrent-primitives)
as a standalone workspace package `torrust-info-hash 0.1.0` in this repository,
then publish it to crates.io. This is part of the plan to move individual types
from the shared `bittorrent-primitives` crate into dedicated packages under this
workspace.

The original migration tracking issue is
[bittorrent-primitives#5](https://github.com/torrust/bittorrent-primitives/issues/5),
which links to the broader infrastructure plan in
[torrust-bittorrent#64](https://github.com/torrust/torrust-bittorrent/issues/64).

## Motivation

The `InfoHash` type is needed by multiple Torrust crates (torrust-tracker, torrust-index, etc.).
Moving it to its own published crate under this workspace allows it to be versioned
and consumed independently, rather than pulling in the entire `bittorrent-primitives` crate.

## Background

- Source: `src/info_hash.rs` from `torrust/bittorrent-primitives` (main branch).
- Author: **Jose Celano** (`josecelano@gmail.com`), the original implementation author.
- The module was copied with workspace-necessary adaptations only — the core
  implementation is unchanged from the original.

A handoff agent already completed the initial file copy and adaptation (see the
now-deleted `packages/info-hash/HANDOFF.md`). What remains is finishing the work
and getting it merged.

## What was already done (by handoff agent)

All changes are unstaged on the current `develop` branch:

| File                                  | Description                                                                               |
| ------------------------------------- | ----------------------------------------------------------------------------------------- |
| `packages/info-hash/Cargo.toml`       | Package manifest; crate name `torrust-info-hash` v0.1.0                                   |
| `packages/info-hash/LICENSE-APACHE`   | Apache-2.0 license (matching workspace)                                                   |
| `packages/info-hash/README.md`        | Crate docs with origin info and usage example                                             |
| `packages/info-hash/src/lib.rs`       | Re-exports `InfoHash` from `info_hash` module                                             |
| `packages/info-hash/src/info_hash.rs` | The `InfoHash` type — faithful copy from `bittorrent-primitives` with minimal adaptations |

### Adaptations from the original

The following differences from the original `info_hash.rs` are intentional and necessary:

1. **`#[cfg(feature = "serde")]` gates** — `serde` is optional (default-on) in this workspace,
   so the `Serialize`, `Deserialize`, and `InfoHashVisitor` impls are feature-gated.
2. **`std::convert::From` → `From`** — Rust 2024 edition; qualified paths are unnecessary.
3. **`#[expect(dead_code, reason = "...")]`** on `fixture::gen_seeded_infohash` — public
   fixture API for downstream consumers; the lint is expected.
4. **`#![allow(clippy::module_name_repetitions)]`** in `lib.rs` — workspace lint compliance.
5. **Crate-level:** name `torrust-info-hash`, edition 2024, license Apache-2.0, `thiserror` v2,
   `serde_json` moved to dev-dependencies.

### Verified

- ✅ `cargo check -p torrust-info-hash --all-targets --all-features` passes
- ✅ `cargo nextest run -p torrust-info-hash --all-targets --all-features` — 12/12 tests pass
- ✅ `cargo test --doc -p torrust-info-hash` passes
- ✅ `diff` against upstream `info_hash.rs` confirms only the intentional adaptations above

## Scope

### 1. Review the existing file copies

Verify the adaptations are correct and nothing was missed from the handoff.

### 2. Add to workspace

Add `"packages/info-hash"` to the `[workspace] members` list in the root `Cargo.toml`.

### 3. Update `AGENTS.md`

Add a row for the new `info-hash` package to the package table in `AGENTS.md` (alphabetically
sorted).

### 4. Run full workspace validation

```bash
cargo check --workspace --all-targets --all-features
cargo nextest run --workspace --all-targets --all-features
cargo +nightly fmt --check
cargo clippy --workspace --all-targets --all-features
linter all
```

### 5. Create branch, commit, open PR

- Branch: `feat/add-info-hash-from-bittorrent-primitives`
- Commit type: `feat(info-hash)` or `refactor(info-hash)`
- PR against `develop`
- Preserve original author with `git commit --author="Jose Celano <josecelano@gmail.com>"`

### 6. Merge after CI passes

### 7. Publish `torrust-info-hash 0.1.0` to crates.io

```bash
cargo publish -p torrust-info-hash --dry-run
cargo publish -p torrust-info-hash
```

### 8. Signal back to dependents

Update the relevant tracking issues:

- [bittorrent-primitives#5](https://github.com/torrust/bittorrent-primitives/issues/5) —
  the `info_hash` module has been migrated.
- The tracker and index repos should switch to `torrust-info-hash` from crates.io.

## Implementation Plan

| Step | Task                                                              | Status          |
| ---- | ----------------------------------------------------------------- | --------------- |
| 1    | Review existing file copies and adaptations                       | ⬜ todo         |
| 2    | Add `"packages/info-hash"` to root `Cargo.toml` workspace members | ⬜ todo         |
| 3    | Add `info-hash` row to package table in `AGENTS.md`               | ⬜ todo         |
| 4    | Run full workspace validation (check, test, fmt, clippy, linter)  | ⬜ todo         |
| 5    | Create branch and commit with original author attribution         | ⬜ todo         |
| 6    | Push branch and open PR against `develop`                         | ⬜ todo         |
| 7    | Merge PR after CI passes                                          | ⬜ todo         |
| 8    | Publish `torrust-info-hash 0.1.0` to crates.io                    | ⬜ todo         |
| 9    | Signal tracker repo to switch dependency                          | ↩️ tracker-side |

## Dependencies of the new package

- `binascii` — hex encoding/decoding
- `serde` (optional, default) — serialization/deserialization
- `thiserror` — error derive macros
