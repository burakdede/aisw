## Summary
Describe what changed and why in 2-5 bullets.

- 
- 

## User Impact
What user problem does this solve? Who is affected?

## CLI / UX Changes
List command-surface changes and behavior updates.

- New flags/options:
- Changed defaults:
- New/updated output:
- Error message changes:

## Backward Compatibility
Call out any breaking changes or migration concerns.

- [ ] No breaking changes
- [ ] Breaking change (describe below)

## Implementation Notes
Key design choices and tradeoffs reviewers should know.

## Tests & Validation
Provide exactly what you ran locally.

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
bash tests/completions_smoke.sh
```

Additional validation (if applicable):

```bash
# Example:
# cargo llvm-cov --summary-only --fail-under-lines 10
```

## Manual Verification
Include copy-pastable steps for reviewers.

```bash
# Example:
# aisw init --yes
# aisw add claude work --api-key '...'
# aisw use claude work
# aisw status
```

## Documentation

- [ ] No docs needed
- [ ] Docs updated (README / CONTRIBUTING / command help)

## Security & Privacy Checklist

- [ ] No secrets/tokens/credentials committed
- [ ] Logs and screenshots are redacted
- [ ] Sensitive output paths are reviewed

## Release Notes
One short line suitable for changelog/release notes.

## Linked Issues
Closes #...

