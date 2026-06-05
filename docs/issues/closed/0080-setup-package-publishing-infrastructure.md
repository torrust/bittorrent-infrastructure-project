---
doc-type: issue
issue-type: task
status: closed
priority: p2
github-issue: 80
spec-path: docs/issues/closed/0080-setup-package-publishing-infrastructure.md
branch: chore/setup-package-publishing-infrastructure
related-pr: 81
last-updated-utc: 2026-06-05 11:30
semantic-links:
  skill-links:
    - create-adr
    - add-new-skill
    - write-markdown-docs
  related-artifacts:
    - Cargo.toml
    - packages/
    - docs/adrs/
    - .github/skills/
---

# Setup Package Publishing Infrastructure

## Summary

Establish the basic workspace configuration and conventions for managing and publishing crates
from the `torrust-bittorrent` workspace. This is a prerequisite for importing the updated
`torrust-tracker-contrib-bencode` crate (issue #64) and for publishing any crate to crates.io.

## Motivation

The workspace currently has `publish = false` as a placeholder and a single shared version
(`1.0.0-alpha.1`) for all crates. Before importing or releasing any crate we need:

- Agreed naming conventions, informed by the decisions made in the Torrust Tracker overhaul
  epic (torrust/torrust-tracker#1669, specifically DEC-02, DEC-03, DEC-04).
- Independent per-crate versioning so that each crate can evolve and be released separately.
- A documented release workflow so contributors know exactly how to publish a new version.
- An ADR capturing the reasoning behind these choices.

## Background — applicable decisions from torrust/torrust-tracker DECISIONS.md

| Decision | Summary                                                                                                                        |
| -------- | ------------------------------------------------------------------------------------------------------------------------------ |
| DEC-02   | Use `torrust-` as the default prefix for Torrust organisation crates (not `torrust-bittorrent-`).                              |
| DEC-03   | The prefix indicates ownership/subdomain, not expected reusability.                                                            |
| DEC-04   | Package folder names match the crate name with the ownership prefix removed (e.g. crate `torrust-bencode` → folder `bencode`). |

Consequence for this repo: folder names under `packages/` are already correct and stay
unchanged. Only the `name` field inside each crate's `Cargo.toml` changes.

## Scope

### 1. Define and document the publishing strategy

Not all crates will be published immediately. The strategy is:

- Publish a crate only when it is actively used by another Torrust project (e.g. the tracker).
  Crates that are not yet consumed externally stay unpublished until they are needed.
- Rationale: these crates were forked from an unmaintained upstream
  ([GGist/bip-rs](https://github.com/GGist/bip-rs)); old versions under the original crate names
  already exist on crates.io. Publishing prematurely under new names adds noise before the code
  is validated in production use.
- Keep `publish = false` in `[workspace.package]` as the safe workspace default.
  Each crate that is ready to publish overrides it with `publish = true` in its own `Cargo.toml`.

**Outcomes of this task:**

1. Update `[workspace.package]` in the root `Cargo.toml`: ensure `publish = false` is present
   and add an inline comment explaining the publish-on-demand strategy, e.g.:

   ```toml
   # publish = false is the workspace default.
   # Each crate in packages/ overrides this with publish = true only when it is
   # actively consumed by another Torrust project and ready for a public release.
   publish = false
   ```

2. Document the rationale in the ADR (see task 4).

### 2. Rename all crates with the `torrust-` prefix

Apply DEC-02 and DEC-04: rename the `name` field in each `packages/<name>/Cargo.toml`.
Folder names under `packages/` do **not** change.

| Current crate name | New crate name      | Folder (unchanged)   |
| ------------------ | ------------------- | -------------------- |
| `bencode`          | `torrust-bencode`   | `packages/bencode`   |
| `dht`              | `torrust-dht`       | `packages/dht`       |
| `disk`             | `torrust-disk`      | `packages/disk`      |
| `handshake`        | `torrust-handshake` | `packages/handshake` |
| `magnet`           | `torrust-magnet`    | `packages/magnet`    |
| `metainfo`         | `torrust-metainfo`  | `packages/metainfo`  |
| `peer`             | `torrust-peer`      | `packages/peer`      |
| `select`           | `torrust-select`    | `packages/select`    |
| `util`             | `torrust-util`      | `packages/util`      |

All inter-crate `[dependencies]` references must be updated to use the new crate names.
All `use`/`extern crate` references inside source files (if any) must be updated too.

### 3. Set an initial version for each crate

Each crate in `packages/` must have an explicit `version` field in its own `Cargo.toml` rather
than inheriting a shared workspace version.

Version choice:

- Each crate starts at `0.1.0` as its initial workspace version, regardless of any prior
  crates.io history under different names. The old `torrust-tracker-contrib-bencode` name and
  its `3.0.0` version belong to a different crate identity; the renamed `torrust-bencode` is a
  fresh publication under a new name.
- When a crate is first published to crates.io its version at that point becomes its first
  public release — the `0.1.0` in the workspace does not need to match any previous history.
- The shared `[workspace.package] version` field is removed (or kept only as a fallback for
  example crates that do not publish).

### 4. Write an ADR for publishing strategy and independent per-crate versioning

Create `docs/adrs/YYYYMMDDHHMMSS_crate_publishing_and_versioning.md` and register it in
`docs/adrs/index.md` documenting:

- **Context**: this is a library workspace forked from an unmaintained upstream
  ([GGist/bip-rs](https://github.com/GGist/bip-rs)); old versions under the original crate names
  already exist on crates.io. Each crate has an independent API surface and release cadence.
- **Decisions**:
  - Every crate in `packages/` carries its own `version` field starting at `0.1.0`; the shared
    `[workspace.package] version` is not used for publishable crates.
  - `publish = false` is the workspace default. A crate is only published to crates.io when it
    is actively consumed by another Torrust project and has been validated in production use.
- **Rationale**: independent versioning avoids coupling unrelated crates; follows crates.io
  conventions for library crates; keeps the changelog and release history per-crate.
  Publish-on-demand avoids polluting crates.io with premature releases of unvalidated forks.
- **Tradeoffs**: a bit more `Cargo.toml` maintenance overhead; no global "this is the workspace
  version" signal; contributors must remember to flip `publish = true` when a crate is ready.
- **References**: DEC-02, DEC-03, DEC-04 from torrust/torrust-tracker DECISIONS.md;
  torrust/torrust-bittorrent#80.

### 5. Create a release skill

Add `.github/skills/dev/maintenance/release-package/SKILL.md` documenting the end-to-end steps
to release a new version of a crate:

1. Determine the new version following SemVer.
2. Update the `version` field in `packages/<name>/Cargo.toml`.
3. Update `CHANGELOG.md` inside the package (if present).
4. Run all checks: `linter all`, `cargo +nightly fmt`, `cargo clippy`, `cargo nextest run`.
5. Commit following Conventional Commits format:
   `chore(release): release torrust-<name> vX.Y.Z`.
6. Open a PR, get it merged to `develop`.
7. Tag the merge commit: `torrust-<name>-vX.Y.Z`.
8. Publish to crates.io: `cargo publish -p torrust-<name>`.
9. Open a GitHub release pointing to the crate tag (e.g. `torrust-bencode-v0.1.0`).
   Each crate gets its own independent GitHub release; multiple releases can coexist
   in the same repository, one per crate per version.

## Acceptance Criteria

- [x] All crates in `packages/` are renamed to `torrust-<name>` in their `Cargo.toml`.
- [x] Folder names under `packages/` are unchanged.
- [x] All inter-crate dependency references use the new crate names.
- [x] Each crate in `packages/` has an explicit `version` field independent of the workspace
      default.
- [x] `[workspace.package] publish = false` is kept as the default; crates ready to publish
      override it with `publish = true` in their own `Cargo.toml`.
- [x] `docs/adrs/20260605103740_crate_publishing_and_versioning.md` and `docs/adrs/index.md`
      exist, cover both the publish-on-demand strategy and independent versioning, and are complete.
- [x] `.github/skills/dev/maintenance/release-package/SKILL.md` exists and covers all release
      steps.
- [x] `cargo check --workspace --all-targets --all-features` passes.
- [x] `linter all` passes.

## Out of scope

- Copying the updated `torrust-tracker-contrib-bencode` source code (issue #64).
- Publishing any crate to crates.io as part of this issue.
- Renaming the repository or moving crates between workspaces.

## Related

- #64 — Import `torrust-tracker-contrib-bencode` (blocked on this issue).
- torrust/torrust-tracker#1669 — Overhaul packages epic (source of DEC-02, DEC-03, DEC-04).
- <https://crates.io/crates/torrust-tracker-contrib-bencode> — v3.0.0.
- <https://github.com/torrust/torrust-tracker/blob/develop/docs/issues/open/1669-overhaul-packages/DECISIONS.md>
