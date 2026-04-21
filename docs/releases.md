# Releasing aisw

## Tag naming convention

Releases follow [Semantic Versioning](https://semver.org/): `vMAJOR.MINOR.PATCH`.

- `v1.0.0`  -  first stable release
- `v1.0.1`  -  patch (bug fixes only)
- `v1.1.0`  -  minor (new features, backwards-compatible)
- `v2.0.0`  -  major (breaking changes)

Pre-release suffixes are not supported by the release workflow; use a separate test tag
(see below) when you need to validate the pipeline without publishing.

## Cutting a release

1. Ensure `main` is clean and all CI checks pass.

2. Update the version in `Cargo.toml`:
   ```toml
   version = "1.0.0"
   ```

3. Commit and push:
   ```sh
   git add Cargo.toml Cargo.lock
   git commit -m "chore: bump version to 1.0.0"
   git push origin main
   ```

4. Push the tag:
   ```sh
   git tag v1.0.0
   git push origin v1.0.0
   ```

5. The release workflow starts automatically. Monitor it at
   `https://github.com/burakdede/aisw/actions/workflows/release.yml`.

6. Once all five build jobs pass, open the draft release on the
   [Releases page](https://github.com/burakdede/aisw/releases), review the
   generated changelog, and click **Publish release**.
7. After publishing, the Homebrew workflow
   (`.github/workflows/homebrew-release.yml`) runs automatically and updates
   `burakdede/homebrew-tap` `Formula/aisw.rb` for that version.

## What the release workflow does

`.github/workflows/release.yml` runs on every `v*.*.*` tag push:

| Job | What it does |
| --- | --- |
| Build (5×) | Compiles `--release --locked` for each target; uses `cross` for `aarch64-unknown-linux-gnu` |
| Size check | Fails the build if the stripped binary exceeds 10 MB |
| Cold start check | Runs `aisw --version` via Python timing; warns (not fails) if > 50 ms |
| Checksum | Generates a `.sha256` file containing the SHA-256 hex digest |
| Upload | Uploads binary + checksum as a GitHub Actions artifact (TTL: 1 day) |
| Release | Downloads all artifacts; creates a **draft** GitHub Release with release notes and all assets |

The release is always created as a draft. A human must publish it.

## Homebrew publishing

Homebrew is published by a separate workflow:

- Trigger: GitHub Release `published` event
- Workflow: `.github/workflows/homebrew-release.yml`
- Target tap repository: `burakdede/homebrew-tap`
- Updated file: `Formula/aisw.rb`

Required repository secret in `burakdede/aisw`:

- `HOMEBREW_TAP_GITHUB_TOKEN` (PAT with write access to `burakdede/homebrew-tap`)

After that job succeeds, users can install with:

```sh
brew tap burakdede/tap
brew install aisw
```

## Build targets

| Target triple | Runner | Method |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` | `cargo build` |
| `aarch64-unknown-linux-gnu` | `ubuntu-latest` | `cross build` |
| `x86_64-apple-darwin` | `macos-latest` | `cargo build` |
| `aarch64-apple-darwin` | `macos-latest` | `cargo build --target` |
| `x86_64-pc-windows-msvc` | `windows-latest` | `cargo build` |

## Binary naming

Assets attached to each release follow the pattern:

```
aisw-<target>          # Linux, macOS
aisw-<target>.exe      # Windows
aisw-<target>.sha256   # checksum (all platforms)
```

This matches the names expected by `install.sh`.

## Testing the workflow without publishing

Use `workflow_dispatch` from the Actions tab (no tag required). This runs all
five build jobs but skips the `release` job  -  the release job only runs when
the workflow is triggered by a tag push.

To test the full pipeline including release creation, push a real semver tag
to a private fork or use a test repository. Remember to delete the draft
release and tag afterwards.

## Verifying checksums

After downloading a binary, verify its integrity:

```sh
# Linux
sha256sum -c aisw-x86_64-unknown-linux-gnu.sha256

# macOS
shasum -a 256 -c aisw-aarch64-apple-darwin.sha256
```

The `.sha256` file contains only the hex digest (no filename). Verify manually
if your tool requires the `hash  filename` format:

```sh
echo "$(cat aisw-x86_64-unknown-linux-gnu.sha256)  aisw-x86_64-unknown-linux-gnu" | sha256sum -c
```
