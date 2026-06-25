# Zigbuild Release CI Design

## Goal

Add a GitHub Actions release workflow that runs when a version tag is pushed and builds release artifacts for:

- `pandar-app` as the `pandar` CLI binary.
- `pandar-network-plugin` as the Bambu Studio network plugin dynamic library.

The workflow must cover Linux, Windows, and macOS, with amd64 and arm64 targets for each platform.

## Trigger And Release Boundary

The workflow has two triggers:

- `push` tags matching `v*`.
- `workflow_dispatch`.

Tag pushes create or update the matching GitHub Release and upload one archive per target.

The workflow does not run on normal branch pushes or pull requests. Existing check workflows remain responsible for regular CI.

The workflow also supports `workflow_dispatch` for pre-tag validation. Manual runs build the six archives and upload them as Actions artifacts only. The GitHub Release publish job is guarded with:

```yaml
if: startsWith(github.ref, 'refs/tags/v')
```

This prevents branch-based manual runs from creating or updating a release accidentally.

## Build Matrix

Use separate CLI and plugin targets when static CLI requirements differ from dynamic-library plugin requirements:

| Artifact label  | Platform | Architecture | Runner           | CLI target                   | Plugin target                |
| --------------- | -------- | ------------ | ---------------- | ---------------------------- | ---------------------------- |
| `linux-amd64`   | Linux    | amd64        | `ubuntu-latest`  | `x86_64-unknown-linux-musl`  | `x86_64-unknown-linux-gnu`   |
| `linux-arm64`   | Linux    | arm64        | `ubuntu-latest`  | `aarch64-unknown-linux-musl` | `aarch64-unknown-linux-gnu`  |
| `windows-amd64` | Windows  | amd64        | `ubuntu-latest`  | `x86_64-pc-windows-gnu`      | `x86_64-pc-windows-gnu`      |
| `windows-arm64` | Windows  | arm64        | `ubuntu-latest`  | `aarch64-pc-windows-gnullvm` | `aarch64-pc-windows-gnullvm` |
| `macos-amd64`   | macOS    | amd64        | `macos-26-intel` | `x86_64-apple-darwin`        | `x86_64-apple-darwin`        |
| `macos-arm64`   | macOS    | arm64        | `macos-26`       | `aarch64-apple-darwin`       | `aarch64-apple-darwin`       |

Linux and Windows matrix jobs run on Ubuntu GitHub-hosted runners. macOS matrix jobs run on GitHub-hosted macOS 26 runners so the Apple SDK is provided by the runner instead of attempting to vendor or fetch an SDK on Linux. GitHub documents `macos-26` as the standard arm64 macOS runner and `macos-26-intel` as the standard x64 macOS runner.

The build matrix uses `fail-fast: false` so all target failures are visible in one tag run.

`cargo-zigbuild` and Zig provide the cross-linker path. The plugin also compiles a C++ shim through the Rust `cc` build dependency, so Linux and Windows plugin builds create wrapper scripts:

```bash
zig-cc-plugin:  exec zig cc  -target "$ZIG_PLUGIN_TARGET" "$@"
zig-cxx-plugin: exec zig c++ -target "$ZIG_PLUGIN_TARGET" "$@"
```

The plugin build exports `CC`, `CXX`, and `AR` to those wrappers for the `cargo zigbuild -p pandar-network-plugin` command. `ZIG_PLUGIN_TARGET` is derived from the plugin target:

| Plugin target                | Zig target            |
| ---------------------------- | --------------------- |
| `x86_64-unknown-linux-gnu`   | `x86_64-linux-gnu`    |
| `aarch64-unknown-linux-gnu`  | `aarch64-linux-gnu`   |
| `x86_64-pc-windows-gnu`      | `x86_64-windows-gnu`  |
| `aarch64-pc-windows-gnullvm` | `aarch64-windows-gnu` |

macOS jobs use the runner Apple C/C++ toolchain for the shim while still invoking `cargo zigbuild`. The amd64 job runs on `macos-26-intel`; the arm64 job runs on `macos-26`. Each macOS job builds its native architecture target rather than cross-building the opposite macOS architecture.

## Static Binary Semantics

`pandar-app` is built with `cargo zigbuild --release -p pandar-app --bin pandar --target "$CLI_TARGET"`.

Linux CLI targets use musl to produce static Linux binaries:

- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-musl`

Windows CLI targets are built with static CRT enabled through `RUSTFLAGS=-C target-feature=+crt-static`:

- `x86_64-pc-windows-gnu`
- `aarch64-pc-windows-gnullvm`

macOS CLI targets are ordinary release Mach-O binaries. They are not fully static because macOS system library and SDK linking does not support the same fully static model as Linux musl.

## Plugin Semantics

`pandar-network-plugin` is built with `cargo zigbuild --release -p pandar-network-plugin --target "$PLUGIN_TARGET"`.

The plugin output is a dynamic library:

- Linux: `libpandar_network_plugin.so`
- Windows: `pandar_network_plugin.dll`
- macOS: `libpandar_network_plugin.dylib`

The workflow must not silently skip plugin targets. If a target cannot build, the release workflow fails and exposes the unsupported target clearly.

Linux plugin targets intentionally use GNU targets rather than musl because the current plugin export map is scoped to `target_env == "gnu"` and the plugin is a dynamic Bambu Studio replacement library, not a static CLI binary. This task does not redesign the plugin ABI export mechanism; release CI is allowed to surface target incompatibilities as build failures.

The previously observed aarch64 GNU linker failure happened under the Nix package path using `ld.bfd`. This release workflow uses `cargo-zigbuild` and Zig's linker path for the Linux aarch64 plugin target. If Zig's linker path still rejects the current Rust `cdylib` plus C++ shim export strategy, the tag release job should fail rather than upload an incomplete release.

Windows and macOS plugin targets rely on the platform dynamic-library export behavior produced by the existing Rust `cdylib` plus C++ shim. This task does not add Windows `.def` files, macOS exported-symbol lists, signing, or notarization. If a Windows or macOS target does not expose or link the required Bambu Studio ABI symbols, the release workflow should fail at build or later compatibility validation rather than hide the limitation.

Release CI does not publish partial GitHub Releases. Each matrix row builds an archive and checksum, then uploads them as GitHub Actions artifacts. A separate `publish` job runs after the full matrix with `needs: [build]`; it downloads all matrix artifacts and calls `softprops/action-gh-release` once. If any target fails, `publish` does not run and no new release assets are uploaded.

Each matrix row uploads one Actions artifact named:

```text
pandar-release-${{ matrix.label }}
```

The artifact contains the target archive and its `.sha256` file. The `publish` job downloads all build artifacts with:

```yaml
path: dist
pattern: pandar-release-*
merge-multiple: true
```

It then uploads `dist/*.tar.gz` and `dist/*.tar.gz.sha256` to the GitHub Release.

## Archive Layout

Each target produces one compressed archive named:

```text
pandar-release-${tag}-${target}.tar.gz
```

`tag` comes from `${{ github.ref_name }}`. `target` is the matrix artifact label (`linux-amd64`, `linux-arm64`, `windows-amd64`, `windows-arm64`, `macos-amd64`, or `macos-arm64`), not necessarily a single Rust target triple, because Linux archives combine a musl CLI target with a GNU plugin target.

Each archive contains:

```text
pandar[.exe]
<platform plugin dynamic library>
```

The archive root is flat so users can unpack directly into a staging directory.

Each archive also has a matching SHA256 file named:

```text
pandar-release-${tag}-${target}.tar.gz.sha256
```

The checksum file uses standard `sha256sum` format:

```text
<sha256>  pandar-release-${tag}-${target}.tar.gz
```

## Tooling

The workflow installs:

- Rust stable via `dtolnay/rust-toolchain`.
- Zig `0.15.2` via `mlugg/setup-zig`.
- `cargo-zigbuild` via `cargo install cargo-zigbuild --locked`.
- The selected Rust target via `rustup target add`.
- C/C++ build prerequisites needed by Rust crates with native build scripts.

The workflow uses `softprops/action-gh-release` to create/update the release and upload archives.

The workflow permissions are restricted to:

```yaml
permissions:
  contents: write
```

Third-party actions are pinned to version tags:

- `actions/checkout@v4`
- `actions/upload-artifact@v4`
- `actions/download-artifact@v4`
- `dtolnay/rust-toolchain@stable`
- `mlugg/setup-zig@v2`
- `softprops/action-gh-release@v2`

The workflow concurrency group is keyed by workflow name and tag:

```yaml
concurrency:
  group: release-${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: false
```

## Documentation

Update `docs/roadmap.md` to record the completed release CI support and to keep the remaining plugin compatibility risk visible.

## Acceptance Criteria

- A new GitHub Actions workflow exists for tag releases.
- The workflow triggers only on `v*` tag pushes.
- The workflow also supports manual `workflow_dispatch` for validation runs, but manual branch runs upload Actions artifacts only and do not create or update GitHub Releases.
- The workflow matrix includes all six required platform/architecture targets.
- The workflow uses `fail-fast: false`.
- `pandar-app` is built with `cargo zigbuild` for every target.
- Linux CLI artifacts use musl targets for static binaries.
- Windows CLI artifacts use static CRT flags.
- macOS CLI artifacts are documented as non-fully-static release binaries.
- `pandar-network-plugin` is built with `cargo zigbuild` for every target as a dynamic library.
- Linux plugin artifacts use GNU targets while Linux CLI artifacts use musl targets.
- The workflow configures the C++ shim build path for cross builds instead of relying on the host C++ compiler by accident.
- Each target archive includes the CLI binary and plugin dynamic library.
- Each target archive has a SHA256 checksum file.
- Matrix jobs upload archives as Actions artifacts, and a separate publish job uploads all archives to the GitHub Release only after every target succeeds.
- The release workflow declares `contents: write` permissions explicitly.
- Existing CI workflows remain unchanged except where directly required.
- Local validation covers `nix run nixpkgs#actionlint -- .github/workflows/release.yml`, `cargo zigbuild --release -p pandar-app --bin pandar --target x86_64-unknown-linux-musl` when `cargo-zigbuild` is available, and archive-script shell syntax through `bash -n` or equivalent static checking.
