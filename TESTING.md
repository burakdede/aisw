# Testing Strategy

This project treats test pass/fail as a release-quality signal.

## Layers

- Unit tests (`src/**`): validate pure logic and edge cases.
- Integration tests (`tests/**`): validate command behavior, profile lifecycle, and end-to-end flows in sandboxed environments.
- Shell smoke test (`tests/completions_smoke.sh`): validates completion scripts and CLI wiring.
- Coverage gate (CI): enforces minimum line coverage on Linux.

## Platform Coverage

- Linux + macOS run full build and test suites.
- Windows runs build + library/unit tests to keep platform compatibility visible.
- Unix-only behavior must be guarded with `#[cfg(unix)]` (or narrower target cfgs).

## Reliability Rules

- No dependency on developer machine state (`HOME`, real keychain, real tool binaries).
- Use isolated temp directories and explicit env vars for every test case.
- Avoid wall-clock sleeps in tests when deterministic assertions are possible.
- For tests that mutate global process state (env vars, child-process signaling), serialize with shared test locks.

## Local Validation

Run before opening a PR:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
bash tests/completions_smoke.sh
```

Optional full signal:

```bash
cargo llvm-cov --summary-only --fail-under-lines 85
```

