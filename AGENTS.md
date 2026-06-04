# AGENTS.md

Guidance for AI coding agents working on the **Torrust BitTorrent** project — a
Cargo workspace of BitTorrent library crates written in Rust.

## Project overview

A collection of Rust library crates for building BitTorrent applications, forked
from [GGist/bip-rs](https://github.com/GGist/bip-rs) and reorganised as a Cargo
workspace under the `torrust-bittorrent` umbrella.

| Crate       | Path                 | Description                                              |
| ----------- | -------------------- | -------------------------------------------------------- |
| `bencode`   | `packages/bencode`   | Parsing and converting bencoded data                     |
| `dht`       | `packages/dht`       | BitTorrent Mainline DHT                                  |
| `disk`      | `packages/disk`      | FileSystem interface for managing torrent pieces on disk |
| `handshake` | `packages/handshake` | Standard BitTorrent handshake trait and implementation   |
| `magnet`    | `packages/magnet`    | Parsing and constructing magnet links                    |
| `metainfo`  | `packages/metainfo`  | Parsing and building `.torrent` metainfo files           |
| `peer`      | `packages/peer`      | Peer wire protocol communication                         |
| `select`    | `packages/select`    | Piece selection algorithm                                |
| `util`      | `packages/util`      | Shared utilities used across packages                    |

Examples live under `examples/` (each is its own workspace member).

## Build commands

```sh
# Check all crates compile (fast, no linking)
cargo check --workspace --all-targets --all-features

# Build everything
cargo build --workspace --all-targets --all-features

# Build a single crate
cargo build -p bencode
```

The minimum supported Rust version (MSRV) is defined in `Cargo.toml`
(`rust-version`). Always use the stable toolchain unless you need nightly
features (formatting requires nightly).

## Test commands

Tests use [cargo-nextest](https://nexte.st/). Install it once:

```sh
cargo install cargo-nextest
```

```sh
# Run all tests
cargo nextest run --workspace --all-targets --all-features

# Run documentation tests (not supported by nextest)
cargo test --doc --workspace

# Run tests for a single crate
cargo nextest run -p bencode

# Run a specific test by name
cargo nextest run -p bencode -- <test_name>
```

**All tests must pass before committing.**

## Lint and format checks

Formatting uses `nightly` rustfmt with the config in `rustfmt.toml`
(`max_width = 130`, module-level import granularity).

```sh
# Check formatting (uses nightly)
cargo +nightly fmt --check

# Apply formatting
cargo +nightly fmt

# Run Clippy (strict — all lints are deny-level; see Cargo.toml)
cargo clippy --workspace --all-targets --all-features

# Check documentation compiles without warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --bins --examples --workspace --all-features

# Check for unused dependencies (requires cargo-machete)
cargo machete --with-metadata
```

Fix every warning and error before opening a PR. The CI will fail otherwise.

## Code style guidelines

- **Edition:** Rust 2024.
- **Unsafe code:** allowed with a warning; prefer safe alternatives and document
  any unavoidable `unsafe` blocks.
- **Lint level:** almost everything is `deny`. Do not silence lints with
  `#[allow(...)]` without a brief comment explaining why.
- **Formatting:** run `cargo +nightly fmt` before committing. Line width is 130.
  Imports are grouped `std` → external → crate-local.
- **Clippy pedantic:** enabled. Keep code idiomatic and avoid unnecessary
  complexity.
- **No unused code:** `unused` lint group is `deny`. Remove dead code rather
  than annotating it.

## Security considerations

- Follow OWASP Top 10 guidance when introducing networking or parsing code.
- Avoid `unsafe` unless strictly necessary; document safety invariants when used.
- Do not introduce dependencies that are unmaintained or have known CVEs without
  prior discussion.

## Torrust custom linter

This project uses the [Torrust Linting](https://github.com/torrust/torrust-linting)
tool — a unified CLI that runs Markdown, YAML, TOML, Rust (clippy + rustfmt),
shell, and spell-check linters in one shot.

Install it once:

```sh
cargo install torrust-linting
```

**Run it before every commit and push:**

```sh
linter all
```

The `linter all` command runs every configured linter (markdownlint, yamllint,
taplo, cspell, clippy, rustfmt, shellcheck). Fix all reported issues before
committing. CI will enforce the same checks.

External tools are installed automatically on first run, but you can also
install them manually — see the
[torrust-linting README](https://github.com/torrust/torrust-linting#external-tools-required)
for the full list.

## Commit and PR conventions

- Follow [Conventional Commits](https://www.conventionalcommits.org/):
  `<type>(<scope>): <description>`
- Common types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `ci`,
  `perf`, `build`.
- Scope is optional but helpful (e.g. `bencode`, `dht`, `ci`).
- Keep the subject line ≤ 72 characters.
- Use the body to explain _why_, not just _what_.
- Each commit should leave the workspace in a buildable, all-tests-passing state.
- PR titles must follow the same Conventional Commits format.
- Run `linter all`, `cargo +nightly fmt`, `cargo clippy`, and
  `cargo nextest run` before pushing.

## CI overview

Workflows live in `.github/workflows/`:

| Workflow        | Triggers  | What it does                                                            |
| --------------- | --------- | ----------------------------------------------------------------------- |
| `testing.yaml`  | push / PR | Format, static analysis, build, unit tests (stable + nightly, multi-OS) |
| `coverage.yaml` | push / PR | Generates code coverage report                                          |

All jobs must be green before merging.

## Monorepo tips

- Each `packages/<name>` is an independent Cargo crate; add it to the workspace
  `members` list in the root `Cargo.toml` when creating a new one.
- Inter-crate dependencies use path references (e.g.
  `packages/dht/Cargo.toml` references `util` as `{ path = "../util" }`).
- Benchmarks live alongside the crate they benchmark (`benches/`).
- Integration tests live in the crate's `tests/` directory.
