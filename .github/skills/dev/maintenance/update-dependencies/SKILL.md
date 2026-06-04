---
name: update-dependencies
description: Guide for updating project dependencies in the torrust-bittorrent project. Covers the manual cargo update workflow including branch creation, running checks, committing, and pushing. Distinguishes trivial updates (Cargo.lock only) from breaking-change updates (code rework needed). Use when updating dependencies, running cargo update, or bumping deps. Triggers on "update dependencies", "cargo update", "update deps", or "bump dependencies".
metadata:
  author: torrust
  version: "1.0"
---

# Updating Dependencies

This skill guides you through updating project dependencies for the Torrust BitTorrent project.

Use `.github/skills/dev/maintenance/add-rust-dependency/SKILL.md` when introducing a new crate.
This skill is for updating already-declared dependencies.

Delivery policy:

- Never push directly to `develop` or `main`.
- Merges into `develop` or `main` must go through a PR opened in `torrust/torrust-bittorrent` from a fork branch (`<fork-owner>:<branch>`).
- Remote names are contributor-specific (`josecelano`, `origin`, `torrust`, etc.); use your configured fork remote.

## Update Categories

Before starting, decide which category the update falls into:

| Category     | Description                                  | Branch / Issue                                                 |
| ------------ | -------------------------------------------- | -------------------------------------------------------------- |
| **Trivial**  | `cargo update` only — no code changes needed | Timestamped branch, no issue required                          |
| **Breaking** | Dependency change requires code rework       | If small: same branch. If large: open a separate issue per dep |

Use `cargo update --dry-run` or read the dependency changelog to classify before starting.

## Quick Reference

```bash
# Get a timestamp (YYYYMMDD)
TIMESTAMP=$(date +%Y%m%d)

# Create branch
git checkout develop && git pull --ff-only
git checkout -b "${TIMESTAMP}-update-dependencies"

# Update dependencies
cargo update 2>&1 | tee /tmp/cargo-update.txt

# If Cargo.lock has no changes, nothing to do — stop here.

# Verify
linter all
cargo machete --with-metadata

# Commit and push
git add Cargo.lock
git commit -S -m "chore: update dependencies" -m "$(cat /tmp/cargo-update.txt)"
git push {your-fork-remote} "${TIMESTAMP}-update-dependencies"
```

## Complete Workflow

### Step 1: Create a Branch

Generate a timestamp prefix to avoid branch name conflicts across repeated runs:

```bash
TIMESTAMP=$(date +%Y%m%d)
git checkout develop
git pull --ff-only
git checkout -b "${TIMESTAMP}-update-dependencies"
```

For breaking-change updates that require a tracked issue:

```bash
git checkout -b {issue-number}-update-dependencies
```

### Step 2: Run Cargo Update

```bash
cargo update 2>&1 | tee /tmp/cargo-update.txt
```

If `Cargo.lock` has no changes, there is nothing to update — exit early.

Review `/tmp/cargo-update.txt` to identify any major version bumps that may be breaking.

### Step 3: Handle Breaking Changes

If any updated dependency introduced a breaking API change:

- **Small rework** (a few lines, no design decisions): fix it in this branch and continue.
- **Large rework** (architectural impact or significant effort): revert that specific dependency
  in `Cargo.toml`, keep the other trivial updates, and open a new issue for the breaking
  dependency separately.

```bash
# Revert a single crate to its current locked version to defer it
cargo update --precise {old-version} {crate-name}
```

### Step 4: Verify

```bash
linter all
cargo machete --with-metadata
```

Fix any failures before proceeding.

### Step 5: Commit and Push

```bash
git add Cargo.lock
git commit -S -m "chore: update dependencies" -m "$(cat /tmp/cargo-update.txt)"
git push {your-fork-remote} "${TIMESTAMP}-update-dependencies"
```

### Step 6: Open PR

Target: `torrust/torrust-bittorrent:develop`  
Title: `chore: update dependencies`

## Decision Guide

| Scenario                                       | Action                                                     |
| ---------------------------------------------- | ---------------------------------------------------------- |
| `cargo update` with no code changes            | Trivial — timestamped branch, no issue                     |
| Breaking change, small rework (< 1 hour)       | Fix in the same branch, note in PR description             |
| Breaking change, large rework (> 1 hour)       | Defer: revert that dep, open a separate issue, separate PR |
| Multiple breaking deps, independent migrations | One issue + PR per dependency to keep diffs reviewable     |
