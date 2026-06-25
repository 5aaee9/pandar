# Phase 24 Cross-Platform Release Validation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a release-smoke gate and operator-facing release documentation so Pandar release archives have a documented, testable packaging contract before they are treated as usable.

**Architecture:** Add one focused Rust helper under `tools/release-smoke` that validates packaged archives and packaged plugin exports using the canonical ABI symbol list. Wire it into `.github/workflows/release.yml` before artifact upload, then document release installation and evidence status separately from Phase 23 Studio compatibility evidence.

**Tech Stack:** Rust standard library, `tar`, `flate2`, `sha2`, existing GitHub Actions release workflow, existing Phase 21 ABI symbol file, markdown docs.

---

## File Structure

- Modify `Cargo.toml`: add `exclude = ["tools/release-smoke"]` so the nested helper crate is explicitly outside the root workspace.
- Create `tools/release-smoke/Cargo.toml`: standalone helper crate with its own `[workspace]` table so it can be run directly by manifest path in CI.
- Create `tools/release-smoke/src/main.rs`: CLI that validates archive/checksum/layout, optional CLI startup, and packaged plugin exports.
- Modify `.github/workflows/release.yml`: run the helper after packaging and before `actions/upload-artifact`.
- Create `docs/release-installation.md`: operator-facing release archive, service deployment, Docker Compose, NixOS, plugin replacement, unsupported target, and signing guidance.
- Create `docs/compatibility/release-artifacts.md`: release evidence manifest with status vocabulary and initial `untested` rows.
- Modify `docs/roadmap.md`: record local Phase 24 release-smoke/docs scaffolding and preserve real-host evidence as unverified until manifest rows exist.

## Support And Skip Matrix

The release-smoke checker receives the target label, runner OS, expected CLI filename, expected plugin filename, and archive path. It must fail for missing archive/checksum/layout/plugin export evidence that is expected on the runner. It may skip only checks that cannot run on the current runner, and it must print `SKIP <check>: <reason> (label=<target-label> runner=<runner-os>)`.

| Target label    | Runner             | CLI startup                                        | Plugin export inspection                                                                              |
| --------------- | ------------------ | -------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `linux-amd64`   | Ubuntu             | run `pandar --help`                                | inspect ELF exports with `nm -D` or `readelf -Ws`                                                     |
| `linux-arm64`   | Ubuntu cross-build | skip CLI execution because target arch differs     | inspect ELF exports with `nm -D` or `readelf -Ws`                                                     |
| `windows-amd64` | Ubuntu cross-build | skip CLI execution because PE cannot run on Ubuntu | inspect PE exports with `objdump`, `llvm-objdump`, or `llvm-nm`; fail if no PE inspector is available |
| `windows-arm64` | Ubuntu cross-build | skip CLI execution because PE cannot run on Ubuntu | inspect PE exports with `objdump`, `llvm-objdump`, or `llvm-nm`; fail if no PE inspector is available |
| `macos-amd64`   | macOS Intel        | run `pandar --help`                                | inspect Mach-O exports with `nm -gU`                                                                  |
| `macos-arm64`   | macOS arm64        | run `pandar --help`                                | inspect Mach-O exports with `nm -gU`                                                                  |

Linux arm64 plugin artifacts stay in the release matrix for Phase 24 only if the packaged `libpandar_network_plugin.so` passes export inspection. If this fails in CI, the implementation must either fix the export path or remove/mark that target unsupported before final approval.

## Task 1: Release-Smoke Helper Skeleton And Layout Checks

**Files:**

- Modify: `Cargo.toml`
- Create: `tools/release-smoke/Cargo.toml`
- Create: `tools/release-smoke/src/main.rs`

- [ ] **Step 1: Exclude the helper crate from the root workspace**

In the root `Cargo.toml`, add this entry inside `[workspace]`:

```toml
exclude = [
    "tools/release-smoke",
]
```

Expected: helper lint/test commands are run by explicit `--manifest-path` commands, not accidentally through `cargo clippy --workspace`.

- [ ] **Step 2: Create the helper crate manifest**

Create `tools/release-smoke/Cargo.toml`:

```toml
[package]
name = "pandar-release-smoke"
version = "0.1.0"
edition = "2024"
publish = false

[workspace]

[dependencies]
flate2 = "1.1"
sha2 = "0.10"
tar = "0.4"
tempfile = "3.24"
```

- [ ] **Step 3: Implement argument parsing and archive/checksum validation**

Implement `tools/release-smoke/src/main.rs` with these commands and types:

```rust
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use flate2::read::GzDecoder;
use tar::Archive;

struct Args {
    label: String,
    runner_os: String,
    archive: PathBuf,
    checksum: PathBuf,
    cli_name: String,
    plugin_name: String,
    repo_root: PathBuf,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("release smoke failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    validate_checksum_file(&args)?;
    let stage = unpack_archive(&args)?;
    validate_layout(&args, &stage)?;
    println!("PASS archive layout: {}", args.label);
    Ok(())
}
```

`parse_args` must accept:

```text
--label <target-label>
--runner-os <linux|macos|windows>
--archive <path>
--checksum <path>
--cli-name <filename>
--plugin-name <filename>
--repo-root <path>
```

Return `Err("usage: ...")` when an argument is missing or unknown.

- [ ] **Step 4: Implement checksum sidecar validation**

`validate_checksum_file` must read the `.sha256` sidecar and verify:

- exactly one non-empty line;
- two whitespace-separated fields;
- the first field is 64 lowercase or uppercase hex characters;
- the second field equals the archive file name, not a path.

Then recompute the archive SHA-256 inside the helper using `sha2::Sha256` and fail if it does not match the sidecar digest. Task 3 keeps `shasum -c` in CI as extra evidence, not the only checksum proof.

- [ ] **Step 5: Implement archive unpacking and exact top-level layout validation**

Use `tar` and `flate2` to unpack the archive into a temp directory under `env::temp_dir()/pandar-release-smoke-<pid>-<label>`. Reject entries whose normalized path:

- is absolute;
- contains `..`;
- has more than one path component.

Normalize tar paths by stripping a leading `./` before counting path components. Explicitly ignore the `./` directory entry itself. After unpacking, read the temp directory and require exactly two top-level files: `cli_name` and `plugin_name`. Error if either is missing or any extra file exists.

- [ ] **Step 6: Add helper unit tests**

Add unit tests in `tools/release-smoke/src/main.rs` for:

- checksum sidecar parsing rejects missing lines, path-valued archive names, and non-hex checksum strings;
- checksum validation rejects digest mismatch;
- tar path normalization accepts `./pandar`, rejects `nested/pandar`, rejects `../pandar`, and ignores `./`.

- [ ] **Step 7: Run the helper against a synthetic archive**

Create a temporary archive outside the repo with two files named `pandar` and `libpandar_network_plugin.so`, a matching `.sha256` line, and run:

```bash
cargo run --manifest-path tools/release-smoke/Cargo.toml -- \
  --label linux-amd64 \
  --runner-os linux \
  --archive /tmp/pandar-release-test.tar.gz \
  --checksum /tmp/pandar-release-test.tar.gz.sha256 \
  --cli-name pandar \
  --plugin-name libpandar_network_plugin.so \
  --repo-root .
```

Expected: layout pass and then later export/startup failures until Task 2 adds controllable checks.

## Task 2: CLI Startup And Packaged Plugin Export Checks

**Files:**

- Modify: `tools/release-smoke/src/main.rs`

- [ ] **Step 1: Load the canonical plugin symbol list**

Add `expected_symbols(repo_root: &Path) -> Result<BTreeSet<String>, String>` that reads `docs/superpowers/specs/2026-06-23-phase-21-network-plugin-abi-symbols.txt`, trims lines, and keeps every line starting with `bambu_network_` or `ft_`.

Error if the set is empty.

Add a unit test that feeds sample symbol-file content into the same parser and verifies it keeps only `bambu_network_*` and `ft_*` lines.

- [ ] **Step 2: Add target support classification**

Add:

```rust
enum CliStartup {
    Run,
    Skip(&'static str),
}

enum PluginInspection {
    Elf,
    MachO,
    Pe,
}
```

Add `support_for(label: &str, runner_os: &str) -> Result<(CliStartup, PluginInspection), String>` matching the support matrix in this plan. The function must error if a label is paired with the wrong runner OS, so CI cannot silently skip checks under a mismatched matrix entry.

Add unit tests verifying:

- `support_for("windows-amd64", "linux")` skips CLI startup and requires PE inspection;
- `support_for("linux-amd64", "macos")` returns a label/runner mismatch error.

- [ ] **Step 3: Implement CLI startup**

For `CliStartup::Run`, execute the unpacked CLI with `--help` and require success. For skipped targets, print `SKIP cli-startup: <reason> (label=<label> runner=<runner_os>)`.

- [ ] **Step 4: Implement export collection**

Implement:

```rust
fn exported_symbols(kind: PluginInspection, plugin: &Path) -> Result<BTreeSet<String>, String>
```

Use:

- ELF: first successful command from `nm -D <plugin>` or `readelf -Ws <plugin>`;
- Mach-O: `nm -gU <plugin>`;
- PE: first successful command from `objdump -p <plugin>`, `llvm-objdump -p <plugin>`, or `llvm-nm -g <plugin>`.

Parse stdout by splitting whitespace and collecting tokens that start with `bambu_network_` or `ft_`. For PE `objdump -p`, also accept lines whose final token is a matching export name.

Normalize inspector output tokens by stripping one leading `_` before matching and comparison. This is for inspector output only; the canonical expected symbol file remains unmodified. The normalization is required for Mach-O `nm -gU`, which commonly reports exported C symbols as `_bambu_network_*` and `_ft_*`.

Fail if no inspector command is available or successful for the target's declared inspection kind. On Ubuntu release jobs, GNU `objdump` from the default binutils package is the expected Windows PE inspector; LLVM tools are fallback options.

- [ ] **Step 5: Compare expected and actual packaged exports**

Compare the expected canonical symbol set to exported symbols from the unpacked plugin file. Error with a sorted missing-symbol list if any expected symbol is missing. Print `PASS plugin exports: <count> symbols`.

Add a unit test for export parsing that proves inspector output tokens `_bambu_network_get_version` and `_ft_abi_version` are collected as `bambu_network_get_version` and `ft_abi_version`.

- [ ] **Step 6: Run focused helper checks**

Build a real local package by running:

```bash
cargo build -p pandar-app --bin pandar
cargo build -p pandar-network-plugin
```

Then create a temporary linux-amd64-style archive from `target/debug/pandar` and `target/debug/libpandar_network_plugin.so`, create its `.sha256`, and run the helper with `--runner-os linux`. Expected: checksum/layout pass, CLI startup pass, plugin export pass.

## Task 3: Release Workflow Integration

**Files:**

- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: Define one sanitized release archive stem**

In `Package artifacts`, define the archive stem once and use it for package, smoke, upload, and docs consistency:

```bash
ref_name="${GITHUB_REF_NAME:-manual}"
safe_ref="${ref_name//\//-}"
archive="pandar-release-${safe_ref}-${{ matrix.label }}.tar.gz"
```

For tag releases, this preserves the spec contract `pandar-release-<tag>-<target-label>.tar.gz`. For `workflow_dispatch` on a branch, it uses the sanitized ref name so branch names with `/` do not create nested paths.

- [ ] **Step 2: Add PE inspector preflight for Windows labels**

Before `Smoke release artifact`, add:

```yaml
- name: Check Windows PE inspector
  if: startsWith(matrix.label, 'windows-')
  shell: bash
  run: |
    set -euo pipefail
    command -v objdump
```

This documents that GNU `objdump` is the expected PE export inspector on Ubuntu cross-build release jobs.

- [ ] **Step 3: Add checksum verification and release-smoke step before upload**

After `Package artifacts` and before `Upload build artifact`, add:

```yaml
- name: Smoke release artifact
  shell: bash
  run: |
    set -euo pipefail
    ref_name="${GITHUB_REF_NAME:-manual}"
    safe_ref="${ref_name//\//-}"
    archive="dist/pandar-release-${safe_ref}-${{ matrix.label }}.tar.gz"
    checksum="$archive.sha256"
    (
      cd dist
      shasum -a 256 -c "$(basename "$checksum")"
    )
    cargo run --manifest-path tools/release-smoke/Cargo.toml -- \
      --label "${{ matrix.label }}" \
      --runner-os "${{ runner.os == 'macOS' && 'macos' || 'linux' }}" \
      --archive "$archive" \
      --checksum "$checksum" \
      --cli-name "${{ matrix.cli-name }}" \
      --plugin-name "${{ matrix.plugin-name }}" \
      --repo-root "$GITHUB_WORKSPACE"
```

- [ ] **Step 4: Keep release publication behavior unchanged**

Do not change the tag trigger, release publication job, artifact names, or `softprops/action-gh-release` configuration except as required by the smoke step.

- [ ] **Step 5: Validate workflow syntax by inspection**

Run:

```bash
sed -n '1,220p' .github/workflows/release.yml
```

Expected: sanitized archive name is used consistently in package and smoke steps; smoke step appears between package and upload; upload paths remain `dist/*`.

## Task 4: Release Installation And Evidence Docs

**Files:**

- Create: `docs/release-installation.md`
- Create: `docs/compatibility/release-artifacts.md`
- Modify: `docs/development.md`

- [ ] **Step 1: Write release installation guide**

Create `docs/release-installation.md` with sections:

- `Release Archive Selection`
- `Checksum Verification`
- `CLI Startup Smoke`
- `Hub, Web, And Agent Deployment`
- `Docker Compose Shapes`
- `NixOS services.pandar`
- `Bambu Studio Plugin Replacement`
- `Unsupported Or Untested Targets`
- `Signing Status`

State the Phase 24 signing decision as `unsigned-accepted`: artifacts remain unsigned for the next release, operators must verify checksums and expect platform warnings, and signing/notarization is deferred.

- [ ] **Step 2: Write release evidence manifest**

Create `docs/compatibility/release-artifacts.md` with:

- allowed status values `passed`, `failed`, `blocked`, `unsupported`, `untested`;
- target rows for `linux-amd64`, `linux-arm64`, `windows-amd64`, `windows-arm64`, `macos-amd64`, `macos-arm64`;
- columns: run/tag, target, archive filename, checksum, layout, CLI startup, plugin exports, real host install, signing, notes;
- initial status values `untested` for real host install rows unless evidence exists;
- note that CI release-smoke evidence and real host installation evidence are distinct.

- [ ] **Step 3: Link docs from development docs**

In `docs/development.md`, add a short release packaging reference near existing build/release/plugin development content:

```markdown
Release packaging references:

- `docs/release-installation.md`
- `docs/compatibility/release-artifacts.md`
```

## Task 5: Roadmap Update

**Files:**

- Modify: `docs/roadmap.md`

- [ ] **Step 1: Add Phase 24 completion bullets without marking real host evidence complete**

Under Phase 24, add bullets that Phase 24 local scaffolding now includes release-smoke validation, packaged plugin export checks, release installation docs, release evidence manifest, and an explicit unsigned-artifact decision.

Add a separate bullet that real host installation evidence remains unverified until `docs/compatibility/release-artifacts.md` records target-family rows.

- [ ] **Step 2: Keep exit criteria intact**

Do not rewrite Phase 24 exit criteria as completed unless real target evidence exists.

## Task 6: Final Verification And Review Inputs

**Files:**

- No new files.

- [ ] **Step 1: Run focused checks**

Run:

```bash
cargo fmt --check
cargo clippy --workspace
cargo fmt --check --manifest-path tools/release-smoke/Cargo.toml
cargo clippy --manifest-path tools/release-smoke/Cargo.toml -- -D warnings
cargo test --manifest-path tools/release-smoke/Cargo.toml
cargo test -p pandar-network-plugin
```

Expected: all pass.

- [ ] **Step 2: Run release-smoke dry run against local debug artifacts**

Build local artifacts, package them into `/tmp/pandar-release-local-linux-amd64.tar.gz`, create a checksum, and run:

```bash
cargo run --manifest-path tools/release-smoke/Cargo.toml -- \
  --label linux-amd64 \
  --runner-os linux \
  --archive /tmp/pandar-release-local-linux-amd64.tar.gz \
  --checksum /tmp/pandar-release-local-linux-amd64.tar.gz.sha256 \
  --cli-name pandar \
  --plugin-name libpandar_network_plugin.so \
  --repo-root .
```

Expected: checksum/layout, CLI startup, and plugin export checks pass.

- [ ] **Step 3: Run broad verification**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
npm --prefix frontend run build
git diff --check
```

Expected: all pass.

- [ ] **Step 4: Prepare final SDD review evidence**

Collect:

- spec path;
- plan path;
- base/head diff;
- verification output;
- note that real host installation evidence remains `untested`.
