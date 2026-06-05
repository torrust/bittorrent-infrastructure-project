---
name: release-package
description: End-to-end workflow for releasing a new version of a crate from the torrust-bittorrent workspace to crates.io. Covers version bumping, pre-flight checks, first-time publish setup, tagging, publishing, and GitHub release creation. Use when releasing any crate in packages/. Triggers on "release crate", "publish package", "bump version", "release torrust-<name>".
metadata:
  author: torrust
  version: "1.0"
  semantic-links:
    related-artifacts:
      - AGENTS.md
      - Cargo.toml
      - docs/adrs/20260605103740_crate_publishing_and_versioning.md
      - .github/skills/dev/git-workflow/create-feature-branch/SKILL.md
      - .github/skills/dev/git-workflow/open-pull-request/SKILL.md
      - .github/skills/dev/git-workflow/run-linters/SKILL.md
---

# Releasing a Package

Use this workflow when publishing a new version of a crate from `packages/` to crates.io.

## Publishing Policy (read first)

- **Publish on demand**: only publish a crate when it is actively consumed by another Torrust
  project and has been validated in production use.
- **First release**: before the first publish of a crate, change `publish = false` to
  `publish = true` in its `packages/<name>/Cargo.toml`.
- **Independent versions**: each crate is versioned independently. Releasing one crate does not
  require bumping any other crate's version.
- All crates start at `0.1.0`. Follow [SemVer](https://semver.org/) for all subsequent releases.

See `docs/adrs/20260605103740_crate_publishing_and_versioning.md` for the full rationale.

## Skill Links

- `AGENTS.md`
- `.github/skills/dev/git-workflow/create-feature-branch/SKILL.md`
- `.github/skills/dev/git-workflow/open-pull-request/SKILL.md`
- `.github/skills/dev/git-workflow/run-linters/SKILL.md`

## Workflow

### Step 1: Create a release branch

Create a dedicated branch for the release following the branching convention.

```bash
git checkout develop
git fetch origin
git pull --ff-only origin develop
git checkout -b chore/release-torrust-<name>-vX.Y.Z
```

Example: `chore/release-torrust-bencode-v0.2.0`

### Step 2: Determine the new version

Follow [SemVer](https://semver.org/):

| Change type                                                  | Version bump              |
| ------------------------------------------------------------ | ------------------------- |
| Bug fix, documentation, internal refactor with no API change | PATCH (`0.1.0` → `0.1.1`) |
| New backwards-compatible API                                 | MINOR (`0.1.0` → `0.2.0`) |
| Breaking API change                                          | MAJOR (`0.1.0` → `1.0.0`) |

Check the current version:

```bash
grep '^version' packages/<name>/Cargo.toml
```

### Step 3: Enable publishing (first release only)

If this is the first public release of the crate, flip the publish flag in
`packages/<name>/Cargo.toml`:

```toml
# Before:
publish = false  # override with publish = true when ready for a public release

# After:
publish = true
```

### Step 4: Bump the version

Update the `version` field in `packages/<name>/Cargo.toml`:

```toml
version = "X.Y.Z"
```

If other crates in the workspace depend on the released crate, update their `Cargo.toml` to
reference the new version (if using a specific version rather than a path dependency).

### Step 5: Update the changelog

If `packages/<name>/CHANGELOG.md` exists, add an entry for the new version:

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Added

- ...

### Changed

- ...

### Fixed

- ...
```

If no `CHANGELOG.md` exists yet, create one following the
[Keep a Changelog](https://keepachangelog.com/) format.

### Step 6: Dry-run publish check

Verify the crate packages correctly before committing:

```bash
cargo publish -p torrust-<name> --dry-run
```

Fix any errors before proceeding.

### Step 7: Run all quality checks

```bash
linter all
cargo +nightly fmt
cargo clippy --workspace --all-targets --all-features
cargo nextest run --workspace --all-targets --all-features
cargo test --doc --workspace
```

All checks must pass before committing.

### Step 8: Commit the release

```bash
git add packages/<name>/Cargo.toml packages/<name>/CHANGELOG.md  # add Cargo.lock too if changed
git commit -m "chore(release): release torrust-<name> vX.Y.Z"
```

### Step 9: Open a PR and get it merged

Open a PR targeting `develop` in `torrust/torrust-bittorrent`. Follow the standard PR workflow
in `.github/skills/dev/git-workflow/open-pull-request/SKILL.md`.

The PR title should match the commit message:
`chore(release): release torrust-<name> vX.Y.Z`

Wait for CI to pass and the PR to be merged before continuing.

### Step 10: Tag the merge commit

After the PR is merged, tag the merge commit on `develop`:

```bash
git checkout develop
git pull --ff-only origin develop
git tag torrust-<name>-vX.Y.Z
git push origin torrust-<name>-vX.Y.Z
```

Tag format: `torrust-<name>-vX.Y.Z` (e.g. `torrust-bencode-v0.2.0`).

Each crate has its own tag namespace; multiple crate tags can coexist on the same commit or
on different commits.

### Step 11: Publish to crates.io

```bash
cargo publish -p torrust-<name>
```

Verify the release appears at `https://crates.io/crates/torrust-<name>`.

### Step 12: Open a GitHub release

Create a GitHub release pointing to the tag:

- **Tag**: `torrust-<name>-vX.Y.Z`
- **Title**: `torrust-<name> vX.Y.Z`
- **Body**: copy the changelog entry for this version.

Each crate has its own independent GitHub release. Multiple releases from different crates can
coexist in the same repository.

## Constraints

- Do not publish a crate that still has `publish = false` in its `Cargo.toml`.
- Do not skip the `--dry-run` step; it catches packaging errors before they reach crates.io.
- Do not publish from a feature branch; always publish from the tag on `develop`.
- Do not force-push or amend the tagged commit after publishing.
- Do not bump versions of crates that are not being released in this PR.

## Related Skills

- Create a feature branch: `.github/skills/dev/git-workflow/create-feature-branch/SKILL.md`
- Open a pull request: `.github/skills/dev/git-workflow/open-pull-request/SKILL.md`
- Run linters: `.github/skills/dev/git-workflow/run-linters/SKILL.md`
