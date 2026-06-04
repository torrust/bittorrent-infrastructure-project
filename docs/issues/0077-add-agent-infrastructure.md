---
doc-type: issue
issue-type: task
status: in-review
priority: p2
github-issue: 77
spec-path: docs/issues/0077-add-agent-infrastructure.md
branch: chore/add-agent-infrastructure
related-pr: null
last-updated-utc: 2026-06-04 16:00
semantic-links:
  skill-links:
    - add-new-skill
    - run-linters
    - write-markdown-docs
  related-artifacts:
    - AGENTS.md
    - .github/skills/
    - docs/skills/semantic-skill-link-convention.md
    - .github/workflows/testing.yaml
---

# Add Agent Infrastructure

## Summary

Set up a complete AI agent infrastructure for the `torrust-bittorrent` workspace, enabling AI
coding agents to work effectively on the project with clear conventions, reusable skills, and
automated linting checks.

## Motivation

The project lacked guidance for AI coding agents. Without a structured set of instructions,
conventions, and reusable skill protocols, agent-assisted development is inconsistent and
error-prone. This task establishes the foundation for reliable agent-assisted workflows across
all crates in the workspace.

## Scope

### 1. AGENTS.md

Add a root-level `AGENTS.md` file that describes the project for AI coding agents. Covers:

- Project overview (crate map)
- Build, test, lint, and format commands
- Code style guidelines (edition 2024, Clippy pedantic, `deny` lints)
- Commit and PR conventions (Conventional Commits)
- CI overview
- Monorepo tips

### 2. Linter configuration files

Add unified linter configuration so that `linter all` (from
[torrust-linting](https://github.com/torrust/torrust-linting)) runs all checks in one command:

- `.markdownlint.yaml` — Markdown style rules (MD049 underscore emphasis)
- `.yamllint.yaml` — YAML style rules
- `rustfmt.toml` — Rust formatter config (`max_width = 130`)
- `project-words.txt` — Custom cspell dictionary for project-specific terms

### 3. Fix all linting issues

Resolve all linting errors reported by `linter all` across the workspace:

- Fix `README.md:71` MD049 emphasis style (`*(only)*` → `_(only)_`)
- Fix TOML formatting issues in `Cargo.toml` files
- Fix YAML issues in workflow files
- Add project-specific words to `project-words.txt`

### 4. Update CI workflow

Replace the `format` job in `.github/workflows/testing.yaml` with a `lint` job that:

- Installs Node.js (needed by markdownlint)
- Installs the `linter` binary from the torrust-linting git repository
- Runs `linter all` to cover formatting, static analysis, spell-check, and shell linting

Remove the separate `clippy` step from the `check` job (now covered by `linter all`).

### 5. Agent Skills

Add 20 reusable Agent Skills under `.github/skills/`, adapted from `torrust-tracker`:

| Skill                           | Path                                                          |
| ------------------------------- | ------------------------------------------------------------- |
| `add-new-skill`                 | `.github/skills/add-new-skill/`                               |
| `create-feature-branch`         | `.github/skills/dev/git-workflow/create-feature-branch/`      |
| `open-pull-request`             | `.github/skills/dev/git-workflow/open-pull-request/`          |
| `run-linters`                   | `.github/skills/dev/git-workflow/run-linters/`                |
| `link-subissue-to-parent-issue` | `.github/skills/dev/github/link-subissue-to-parent-issue/`    |
| `add-rust-dependency`           | `.github/skills/dev/maintenance/add-rust-dependency/`         |
| `install-linter`                | `.github/skills/dev/maintenance/install-linter/`              |
| `update-dependencies`           | `.github/skills/dev/maintenance/update-dependencies/`         |
| `create-adr`                    | `.github/skills/dev/planning/create-adr/`                     |
| `create-issue`                  | `.github/skills/dev/planning/create-issue/`                   |
| `create-refactor-plan`          | `.github/skills/dev/planning/create-refactor-plan/`           |
| `write-markdown-docs`           | `.github/skills/dev/planning/write-markdown-docs/`            |
| `fetch-review-threads`          | `.github/skills/dev/pr-reviews/fetch-review-threads/`         |
| `process-copilot-suggestions`   | `.github/skills/dev/pr-reviews/process-copilot-suggestions/`  |
| `resolve-review-threads`        | `.github/skills/dev/pr-reviews/resolve-review-threads/`       |
| `review-pr`                     | `.github/skills/dev/pr-reviews/review-pr/`                    |
| `handle-errors-in-code`         | `.github/skills/dev/rust-code-quality/handle-errors-in-code/` |
| `handle-secrets`                | `.github/skills/dev/rust-code-quality/handle-secrets/`        |
| `review-task`                   | `.github/skills/dev/task-reviews/review-task/`                |
| `write-unit-test`               | `.github/skills/dev/testing/write-unit-test/`                 |

Adaptations from the tracker source:

- Repository references updated from `torrust/torrust-tracker` to `torrust/torrust-bittorrent`.
- Hook script calls (`./contrib/dev-tools/git/hooks/pre-commit.sh`) replaced with `linter all`.

### 6. Semantic Skill Link Convention

Add `docs/skills/semantic-skill-link-convention.md` — a lightweight machine-readable convention
for coupling Agent Skills to repository artifacts using `skill-link` markers.

## Acceptance Criteria

- [x] `AGENTS.md` exists at the repository root with accurate project information
- [x] `linter all` passes with no errors across the full workspace
- [x] CI `testing.yaml` uses the torrust linter in a `lint` job
- [x] All 20 skills are present under `.github/skills/` with correct `name` frontmatter
- [x] `docs/skills/semantic-skill-link-convention.md` is present and accurate

## Changes

| Commit    | Description                                                            |
| --------- | ---------------------------------------------------------------------- |
| `72bbadd` | `chore(docs): add AGENTS.md for AI coding agents`                      |
| `027ecc3` | `chore(lint): add linter configuration files`                          |
| `77822cc` | `fix(lint): fix all linting issues reported by linter all`             |
| `1dee8ae` | `ci(lint): replace format job with torrust linter in testing workflow` |
| `45c5dd3` | `chore(agents): add agent skills from torrust-tracker`                 |
| `480a33d` | `docs(skills): add semantic skill link convention`                     |
