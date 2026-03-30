# Contributing

## Setup

After cloning, run this once to activate the local git hooks:

```
git config core.hooksPath .githooks
```

Verify it is active:

```
git config --get core.hooksPath
```

Expected output:

```
.githooks
```

This installs two hooks:
- **pre-commit** — runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo build --release`, `cargo test`, and `bash tests/completions_smoke.sh` when `bash` is available
- **pre-push** — runs `cargo test` and `cargo build --release`

These mirror most of the CI checks so failures are caught locally before they hit GitHub Actions. Coverage and GitHub-hosted matrix-only checks still run in CI.

## Requirements

- Rust 1.80 or later
- `cargo-llvm-cov` for coverage: `cargo install cargo-llvm-cov`

## Build

```
cargo build
cargo build --release
```

## Tests

Run all tests:

```
cargo test
```

Run a specific module's tests:

```
cargo test config::
cargo test profile::
```

Run integration tests only:

```
cargo test --test integration
```

## Coverage

Check coverage locally:

```
cargo llvm-cov
```

Generate an HTML report you can open in a browser:

```
cargo llvm-cov --html
open target/llvm-cov/html/index.html
```

The project targets 80% line coverage overall. Coverage is enforced in CI.

## Lint

```
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Clippy warnings are errors in CI. Fix them before pushing.

## Commit style

Short subject line. No "feat:"/"fix:" prefixes. No bullet lists in the body unless the change genuinely has multiple unrelated parts. Write what changed and why — the diff shows what.

## Pull requests

One logical change per PR. If a PR needs a long explanation, the code probably needs simplification first.
