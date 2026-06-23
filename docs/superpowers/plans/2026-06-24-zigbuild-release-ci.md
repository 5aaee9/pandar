# Zigbuild Release CI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development under `$sdd-workflow` to implement this plan task-by-task with independent review gates. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add tag-driven release CI that builds `pandar` CLI and `pandar-network-plugin` artifacts with `cargo-zigbuild` for Linux, Windows, and macOS on amd64 and arm64.

**Architecture:** Add one GitHub Actions workflow that builds six matrix rows into Actions artifacts, then publishes all artifacts to a GitHub Release only after every row succeeds and only for `v*` tag refs. Keep normal CI untouched. Update roadmap documentation to record release CI support and the remaining plugin compatibility risks.

**Tech Stack:** GitHub Actions, Rust stable, `cargo-zigbuild`, Zig 0.15.2, `softprops/action-gh-release`, Bash packaging.

---

## Files

- Create: `.github/workflows/release.yml`
- Modify: `docs/roadmap.md`

## Task 1: Add Release Workflow

**Files:**
- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Create the workflow file**

Create `.github/workflows/release.yml` with this content:

```yaml
name: Release

on:
  push:
    tags:
      - "v*"
  workflow_dispatch:

permissions:
  contents: write

concurrency:
  group: release-${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: false

jobs:
  build:
    name: Build release artifacts (${{ matrix.label }})
    runs-on: ${{ matrix.runner }}
    timeout-minutes: 90
    strategy:
      fail-fast: false
      matrix:
        include:
          - label: linux-amd64
            runner: ubuntu-latest
            cli-target: x86_64-unknown-linux-musl
            plugin-target: x86_64-unknown-linux-gnu
            zig-plugin-target: x86_64-linux-gnu
            cli-name: pandar
            plugin-name: libpandar_network_plugin.so
            use-zig-cc: true
            windows-crt-static: false
          - label: linux-arm64
            runner: ubuntu-latest
            cli-target: aarch64-unknown-linux-musl
            plugin-target: aarch64-unknown-linux-gnu
            zig-plugin-target: aarch64-linux-gnu
            cli-name: pandar
            plugin-name: libpandar_network_plugin.so
            use-zig-cc: true
            windows-crt-static: false
          - label: windows-amd64
            runner: ubuntu-latest
            cli-target: x86_64-pc-windows-gnu
            plugin-target: x86_64-pc-windows-gnu
            zig-plugin-target: x86_64-windows-gnu
            cli-name: pandar.exe
            plugin-name: pandar_network_plugin.dll
            use-zig-cc: true
            windows-crt-static: true
          - label: windows-arm64
            runner: ubuntu-latest
            cli-target: aarch64-pc-windows-gnullvm
            plugin-target: aarch64-pc-windows-gnullvm
            zig-plugin-target: aarch64-windows-gnu
            cli-name: pandar.exe
            plugin-name: pandar_network_plugin.dll
            use-zig-cc: true
            windows-crt-static: true
          - label: macos-amd64
            runner: macos-26-intel
            cli-target: x86_64-apple-darwin
            plugin-target: x86_64-apple-darwin
            zig-plugin-target: ""
            cli-name: pandar
            plugin-name: libpandar_network_plugin.dylib
            use-zig-cc: false
            windows-crt-static: false
          - label: macos-arm64
            runner: macos-26
            cli-target: aarch64-apple-darwin
            plugin-target: aarch64-apple-darwin
            zig-plugin-target: ""
            cli-name: pandar
            plugin-name: libpandar_network_plugin.dylib
            use-zig-cc: false
            windows-crt-static: false

    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.cli-target }},${{ matrix.plugin-target }}

      - name: Install Zig
        uses: mlugg/setup-zig@v2
        with:
          version: 0.15.2

      - name: Install cargo-zigbuild
        run: cargo install cargo-zigbuild --locked

      - name: Build pandar CLI
        shell: bash
        run: |
          set -euo pipefail
          if [ "${{ matrix.windows-crt-static }}" = "true" ]; then
            export RUSTFLAGS="-C target-feature=+crt-static"
          fi
          cargo zigbuild --release -p pandar-app --bin pandar --target "${{ matrix.cli-target }}"

      - name: Build pandar network plugin
        shell: bash
        run: |
          set -euo pipefail
          if [ "${{ matrix.use-zig-cc }}" = "true" ]; then
            mkdir -p "$RUNNER_TEMP/zig-cc"
            printf '%s\n' '#!/usr/bin/env bash' 'exec zig cc -target "$ZIG_PLUGIN_TARGET" "$@"' > "$RUNNER_TEMP/zig-cc/zig-cc-plugin"
            printf '%s\n' '#!/usr/bin/env bash' 'exec zig c++ -target "$ZIG_PLUGIN_TARGET" "$@"' > "$RUNNER_TEMP/zig-cc/zig-cxx-plugin"
            chmod +x "$RUNNER_TEMP/zig-cc/zig-cc-plugin" "$RUNNER_TEMP/zig-cc/zig-cxx-plugin"
            export ZIG_PLUGIN_TARGET="${{ matrix.zig-plugin-target }}"
            export CC="$RUNNER_TEMP/zig-cc/zig-cc-plugin"
            export CXX="$RUNNER_TEMP/zig-cc/zig-cxx-plugin"
            export AR="zig ar"
          fi
          cargo zigbuild --release -p pandar-network-plugin --target "${{ matrix.plugin-target }}"

      - name: Package artifacts
        shell: bash
        run: |
          set -euo pipefail
          tag="${GITHUB_REF_NAME:-manual}"
          archive="pandar-release-${tag}-${{ matrix.label }}.tar.gz"
          stage="$RUNNER_TEMP/pandar-release-${{ matrix.label }}"
          mkdir -p "$stage" dist
          cp "target/${{ matrix.cli-target }}/release/${{ matrix.cli-name }}" "$stage/${{ matrix.cli-name }}"
          cp "target/${{ matrix.plugin-target }}/release/${{ matrix.plugin-name }}" "$stage/${{ matrix.plugin-name }}"
          tar -C "$stage" -czf "dist/$archive" .
          (
            cd dist
            shasum -a 256 "$archive" | awk '{print $1 "  " $2}' > "$archive.sha256"
          )

      - name: Upload build artifact
        uses: actions/upload-artifact@v4
        with:
          name: pandar-release-${{ matrix.label }}
          path: dist/*
          if-no-files-found: error

  publish:
    name: Publish GitHub Release
    runs-on: ubuntu-latest
    needs:
      - build
    if: startsWith(github.ref, 'refs/tags/v')
    timeout-minutes: 15
    steps:
      - name: Download release artifacts
        uses: actions/download-artifact@v4
        with:
          path: dist
          pattern: pandar-release-*
          merge-multiple: true

      - name: Publish release
        uses: softprops/action-gh-release@v2
        with:
          files: |
            dist/*.tar.gz
            dist/*.tar.gz.sha256
```

- [ ] **Step 2: Run workflow syntax validation**

Run:

```bash
nix run nixpkgs#actionlint -- .github/workflows/release.yml
```

Expected: command exits `0`.

## Task 2: Update Roadmap

**Files:**
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Add release CI completion note**

In `docs/roadmap.md`, add a `Completed` bullet near the existing GitHub Actions/Nix entries:

```markdown
- Added tag-driven GitHub Release CI for `pandar` CLI and `pandar-network-plugin` artifacts using `cargo-zigbuild`, covering Linux, Windows, and macOS on amd64 and arm64 with per-target checksums; macOS CLI artifacts are ordinary release Mach-O binaries rather than fully static binaries.
```

- [ ] **Step 2: Add immediate risk note**

In `docs/roadmap.md` under `Immediate Next`, keep a risk note for plugin compatibility:

```markdown
- Validate the zigbuild release artifacts on real Windows, macOS, and Linux hosts, especially the arm64 network-plugin dynamic-library exports and Bambu Studio loading behavior.
```

## Task 3: Verify Release Workflow Locally

**Files:**
- Read: `.github/workflows/release.yml`
- Read: `docs/roadmap.md`

- [ ] **Step 1: Run actionlint**

Run:

```bash
nix run nixpkgs#actionlint -- .github/workflows/release.yml .github/workflows/checks.yml .github/workflows/hestia-gc.yml
```

Expected: command exits `0`.

- [ ] **Step 2: Run shell syntax check for generated Zig wrapper shape**

Run:

```bash
tmpdir="$(mktemp -d)"
printf '%s\n' '#!/usr/bin/env bash' 'exec zig cc -target "$ZIG_PLUGIN_TARGET" "$@"' > "$tmpdir/zig-cc-plugin"
printf '%s\n' '#!/usr/bin/env bash' 'exec zig c++ -target "$ZIG_PLUGIN_TARGET" "$@"' > "$tmpdir/zig-cxx-plugin"
bash -n "$tmpdir/zig-cc-plugin" "$tmpdir/zig-cxx-plugin"
rm -rf "$tmpdir"
```

Expected: command exits `0`.

- [ ] **Step 3: Run local zigbuild smoke test when available**

Run:

```bash
if command -v cargo-zigbuild >/dev/null 2>&1 || cargo zigbuild --version >/dev/null 2>&1; then
  rustup target add x86_64-unknown-linux-musl
  cargo zigbuild --release -p pandar-app --bin pandar --target x86_64-unknown-linux-musl
else
  echo "cargo-zigbuild unavailable; smoke build skipped"
fi
```

Expected: either the x86_64 Linux musl CLI build exits `0`, or the command prints the skip message because `cargo-zigbuild` is unavailable.

- [ ] **Step 4: Run format and diff checks**

Run:

```bash
cargo fmt --check
git diff --check
```

Expected: both commands exit `0`.
