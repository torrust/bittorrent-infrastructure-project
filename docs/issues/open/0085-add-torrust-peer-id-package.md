---
doc-type: issue
issue-type: task
status: open
priority: p1
github-issue: 85
spec-path: docs/issues/open/0085-add-torrust-peer-id-package.md
branch: feat/add-torrust-peer-id
related-pr: null
last-updated-utc: 2026-06-08 00:00
semantic-links:
  skill-links:
    - release-package
  related-artifacts:
    - packages/peer-id/Cargo.toml
    - packages/peer-id/src/
    - Cargo.toml
  external-links:
    - https://github.com/torrust/torrust-tracker/issues/1884
    - https://crates.io/crates/torrust-peer-id
---

# Add `torrust-peer-id` Package

## Summary

Migrate the `peer-id` package from [torrust/torrust-tracker](https://github.com/torrust/torrust-tracker)
into this workspace as `torrust-peer-id 0.1.0`, then publish it to crates.io so that the tracker
can replace its path dependency with a versioned crate reference.

## Motivation

The `peer-id` crate lives inside the tracker workspace as a path dependency used by
`packages/http-protocol`, `packages/primitives`, and `packages/udp-protocol`. It belongs in
this repository — the canonical home for BitTorrent infrastructure crates — so it can be
published, versioned, and consumed independently.

The corresponding tracker-side work is tracked in
[torrust/torrust-tracker#1884](https://github.com/torrust/torrust-tracker/issues/1884).

## Background

- Source: `packages/peer-id` in `torrust/torrust-tracker`, branch
  `1884-1669-move-bittorrent-peer-id-to-torrust-bittorrent`.
- Original crate name in the tracker: `bittorrent-peer-id`.
- The crate is adapted from `aquatic_peer_id 0.9.0` by Joakim Frostegård (Apache-2.0).
  `LICENSE-APACHE` is retained to preserve the required upstream attribution.
- No public API or behaviour changes are made — this is a straight rename + move.

## Scope

### 1. Add `packages/peer-id/` to this workspace

Copy `packages/peer-id` from the tracker branch and adapt:

| File             | Change                                                                                                                                                                  |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Cargo.toml`     | Rename crate `bittorrent-peer-id` → `torrust-peer-id`; set `version = "0.1.0"`; set `publish = true`; remove tracker-specific overrides; add `[lints] workspace = true` |
| `LICENSE`        | Remove — AGPL-3.0 was inherited from tracker workspace; crate now uses `license = "Apache-2.0"` from this workspace                                                     |
| `LICENSE-APACHE` | Keep — required Apache-2.0 attribution for upstream `aquatic_peer_id`                                                                                                   |
| `README.md`      | Update heading from `bittorrent-peer-id` → `torrust-peer-id`                                                                                                            |
| `src/peer_id.rs` | Add `const` modifier to `PeerId::as_bytes` (clippy `missing-const-for-fn`)                                                                                              |
| `src/`           | No other API or behaviour changes                                                                                                                                       |

### 2. Register the crate in the workspace

Add `"packages/peer-id"` to the `[workspace] members` list in the root `Cargo.toml`.

### 3. Publish `torrust-peer-id 0.1.0` to crates.io

Once the PR is merged:

```bash
cargo publish -p torrust-peer-id --dry-run
cargo publish -p torrust-peer-id
```

### 4. Signal back to the tracker repo

After the crate is published, the tracker-side tasks (torrust/torrust-tracker#1884 T7–T14) can proceed:

- Replace the path dep on `packages/peer-id` in `packages/http-protocol`, `packages/primitives`,
  and `packages/udp-protocol` with `torrust-peer-id = "0.1.0"` (udp-protocol also needs
  `features = ["zerocopy"]`).
- Remove `packages/peer-id/` from the tracker workspace.
- Update `AGENTS.md`, `packages/AGENTS.md`, and `docs/packages.md` in the tracker.

## Implementation Plan

| Step | Task                                                            | Status     |
| ---- | --------------------------------------------------------------- | ---------- |
| 1    | Copy and adapt `packages/peer-id/` from tracker branch          | ✅ done    |
| 2    | Add `"packages/peer-id"` to root `Cargo.toml` workspace members | ✅ done    |
| 3    | Fix `clippy::missing-const-for-fn` on `PeerId::as_bytes`        | ✅ done    |
| 4    | Run `cargo check/nextest/clippy/fmt` — all pass                 | ✅ done    |
| 5    | Create branch `feat/add-torrust-peer-id` and commit             | ✅ done    |
| 6    | Push branch and open PR against `develop`                       | ⬜ pending |
| 7    | Merge PR after CI passes                                        | ⬜ pending |
| 8    | Publish `torrust-peer-id 0.1.0` to crates.io                    | ⬜ pending |
| 9    | Signal tracker repo to proceed with removal                     | ⬜ pending |

## Acceptance Criteria

- [ ] `packages/peer-id/Cargo.toml` has `name = "torrust-peer-id"`, `version = "0.1.0"`, `publish = true`.
- [ ] `LICENSE-APACHE` is present in `packages/peer-id/`; no `LICENSE` file exists there.
- [ ] `cargo check --workspace --all-targets --all-features` passes.
- [ ] `cargo nextest run --workspace --all-targets --all-features` passes (255 tests).
- [ ] `cargo clippy --workspace --all-targets --all-features` passes with zero errors/warnings.
- [ ] `cargo +nightly fmt --check` passes.
- [ ] `linter all` passes.
- [ ] `cargo publish -p torrust-peer-id --dry-run` succeeds.
- [ ] Crate is visible at <https://crates.io/crates/torrust-peer-id>.

## Acceptance Verification

### Automatic Checks

```bash
cargo check --workspace --all-targets --all-features
cargo nextest run --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features
cargo +nightly fmt --check
linter all
cargo publish -p torrust-peer-id --dry-run
```

### Manual Verification

| Scenario                   | Command                                           | Expected Result      | Status     | Evidence |
| -------------------------- | ------------------------------------------------- | -------------------- | ---------- | -------- |
| Crate visible on crates.io | Browse <https://crates.io/crates/torrust-peer-id> | Version 0.1.0 listed | ⬜ pending | —        |
| Tracker path dep replaced  | Tracker builds with `torrust-peer-id = "0.1.0"`   | Tracker CI green     | ⬜ pending | —        |

## Out of Scope

- Any API or behaviour changes to the peer-id crate.
- Migrating other tracker packages to this repo (separate issues).
- Updating the tracker workspace to remove the path dep (separate tracker-side issue).

## Related

- [torrust/torrust-tracker#1884](https://github.com/torrust/torrust-tracker/issues/1884) — originating tracker issue.
- [#80](https://github.com/torrust/torrust-bittorrent/issues/80) — package publishing infrastructure (prerequisite).
- [#82](https://github.com/torrust/torrust-bittorrent/issues/82) — import bencode package (prior art for this workflow).
