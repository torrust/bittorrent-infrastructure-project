---
doc-type: adr
status: accepted
last-updated-utc: 2026-06-05 10:37
semantic-links:
  skill-links:
    - release-package
  related-issues:
    - https://github.com/torrust/torrust-bittorrent/issues/80
    - https://github.com/torrust/torrust-bittorrent/issues/64
  related-artifacts:
    - Cargo.toml
    - packages/bencode/Cargo.toml
    - packages/dht/Cargo.toml
    - packages/disk/Cargo.toml
    - packages/handshake/Cargo.toml
    - packages/magnet/Cargo.toml
    - packages/metainfo/Cargo.toml
    - packages/peer/Cargo.toml
    - packages/select/Cargo.toml
    - packages/util/Cargo.toml
    - .github/skills/dev/maintenance/release-package/SKILL.md
    - docs/issues/0080-setup-package-publishing-infrastructure.md
---

# Crate Publishing and Versioning Strategy

- Date: 2026-06-05
- References: [issue #80](https://github.com/torrust/torrust-bittorrent/issues/80),
  [torrust/torrust-tracker DECISIONS.md DEC-02/DEC-03/DEC-04](https://github.com/torrust/torrust-tracker/blob/develop/docs/issues/open/1669-overhaul-packages/DECISIONS.md)

## Description

The `torrust-bittorrent` workspace is a collection of library crates forked from the unmaintained
[GGist/bip-rs](https://github.com/GGist/bip-rs) project. Multiple questions needed to be
answered before any crate could be published or consumed by other Torrust projects:

1. **What should crates be named?** The workspace covers BitTorrent infrastructure. Should the
   prefix be `torrust-bittorrent-` (project-scoped) or just `torrust-` (organisation-scoped)?
2. **When should crates be published?** Old versions under the original GGist names already
   exist on crates.io. Publishing renamed crates prematurely would add noise before they are
   validated in production.
3. **How should versions be managed?** A single shared workspace version couples unrelated
   crates. Independent versioning keeps each crate's release history clean.

## Agreement

### Naming — `torrust-` prefix, folder name without prefix

Adopted from DEC-02 and DEC-04 of the Torrust Tracker overhaul DECISIONS.md:

- Crates are named `torrust-<short-name>` (e.g. `torrust-bencode`, `torrust-dht`).
  The extra `bittorrent` segment adds length without adding meaningful disambiguation.
- Package folder names under `packages/` match the crate name **with the prefix removed**
  (e.g. crate `torrust-bencode` → folder `packages/bencode`). This keeps folder names short
  and directly inferrable from the crate name.
- The prefix indicates Torrust organisation ownership, not expected reusability (DEC-03).
  Tracker-domain crates use `torrust-tracker-`; organisation-level shared crates use `torrust-`.

### Publishing — publish on demand

- `publish = false` is the workspace default (set in `[workspace.package]`).
- Each crate in `packages/` sets `publish = false` explicitly with a comment.
- A crate is only flipped to `publish = true` and released to crates.io when:
  - it is actively consumed by another Torrust project, **and**
  - it has been validated in production use.
- Rationale: these crates are forks of unmaintained upstream code. Publishing before validation
  would pollute crates.io with potentially stale or incomplete crates under new names.

### Versioning — independent per-crate versions

- The shared `version` field is removed from `[workspace.package]`.
- Every crate in `packages/` carries its own explicit `version` field.
- All crates start at `0.1.0` regardless of prior crates.io history under different names
  (old names are different crate identities; their version history does not carry over).
- **Exception — imported crates with an existing published version**: when a crate is imported
  from another Torrust repository where it was already published to crates.io under a _different_
  crate name, the version from that prior publication is preserved in this workspace. This avoids
  confusion for consumers who may be familiar with the prior version number and ensures a clear
  continuity of the release history. The first version published under the _new_ crate name must
  therefore be at least as high as the last version published under the old name.
  Example: `torrust-tracker-contrib-bencode v3.0.0` was imported and renamed to `torrust-bencode`;
  the version in this workspace is kept at `3.0.0`.
- Subsequent releases follow [SemVer](https://semver.org/): PATCH for fixes, MINOR for
  new backwards-compatible API, MAJOR for breaking changes.
- Each crate is released independently; releasing one crate does not require bumping any other.

## Tradeoffs Accepted

- Each `packages/<name>/Cargo.toml` must explicitly declare `version` and `publish` fields
  (slightly more maintenance overhead than inheriting from `[workspace.package]`).
- There is no single "workspace version" signal visible to external consumers; each crate's
  version is the only relevant signal.
- Crates that are not yet published are still importable via path dependencies within the
  workspace, which is the intended usage until a crate is production-validated.

## Affected Code

- `Cargo.toml` — `[workspace.package]` publish and version policy
- `packages/*/Cargo.toml` — per-crate `version` and `publish` fields
- `.github/skills/dev/maintenance/release-package/SKILL.md` — release workflow
