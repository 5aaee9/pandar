# Phase 23 Studio Preflight Helper Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a local, read-only Phase 23 preflight helper that validates Bambu Studio plugin smoke-test prerequisites and prints a redacted evidence row template without claiming real Studio compatibility.

**Architecture:** Create a standalone Rust crate under `tools/studio-plugin-smoke`, following the existing `tools/release-smoke` and `tools/scaled-artifact-smoke` pattern. The tool owns argument parsing, local validation, redacted issue reporting, and evidence-row rendering in one focused `main.rs`; docs explain how to use it before the existing manual Studio checklist.

**Tech Stack:** Rust 2024 standalone binary crate, standard library only plus `tempfile` for tests.

---

## File Structure

- Create `tools/studio-plugin-smoke/Cargo.toml`: standalone crate manifest with `[workspace]` so it does not join the main workspace.
- Create `tools/studio-plugin-smoke/src/main.rs`: CLI parser, validators, output renderer, and unit tests.
- Modify `docs/compatibility/bambu-studio-plugin-smoke.md`: add the preflight step before plugin replacement.
- Modify `docs/compatibility/bambu-studio-plugin.md`: record local preflight helper coverage separately from real Studio evidence.
- Modify `docs/roadmap.md`: add a Phase 23 local readiness milestone while preserving the real Studio blocker.

## Task 1: Add The Standalone Preflight Crate

**Files:**
- Create: `tools/studio-plugin-smoke/Cargo.toml`
- Create: `tools/studio-plugin-smoke/src/main.rs`

- [ ] **Step 1: Create the crate manifest**

Create `tools/studio-plugin-smoke/Cargo.toml`:

```toml
[package]
name = "pandar-studio-plugin-smoke"
version = "0.1.0"
edition = "2024"
publish = false

[workspace]

[dev-dependencies]
tempfile = "3.24"
```

- [ ] **Step 2: Add the initial implementation with tests**

Create `tools/studio-plugin-smoke/src/main.rs` with these units:

```rust
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Debug, Eq, PartialEq)]
struct Args {
    studio_path: PathBuf,
    plugin_artifact: PathBuf,
    hub_url: String,
    frontend_url: String,
    os: TargetOs,
    arch: String,
    studio_version: String,
    test_date: String,
    pandar_commit: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetOs {
    Linux,
    Windows,
    Macos,
}

fn main() -> ExitCode {
    match run() {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<String, String> {
    let args = parse_args(env::args().skip(1).collect())?;
    validate(&args)?;
    Ok(render_output(&args))
}
```

Implement the parser and validators to match the spec:

- First token must be `--preflight`.
- Remaining args must be exact `--name value` pairs.
- Required flags: `--studio-path`, `--plugin-artifact`, `--hub-url`, `--frontend-url`, `--os`, `--arch`, `--studio-version`, `--test-date`, `--pandar-commit`.
- `--os`: `linux`, `windows`, `macos`.
- `--arch`: `x86_64`, `amd64`, `aarch64`, `arm64`.
- Expected plugin filenames:
  - Linux: `libpandar_network_plugin.so`
  - Windows: `pandar_network_plugin.dll`
  - macOS: `libpandar_network_plugin.dylib`
- URL validation: string starts with `http://` or `https://` and the authority portion before the next `/` does not contain `@`.
- Date validation: exact byte pattern `NNNN-NN-NN`.
- Commit validation: non-empty and contains no `/`, `\`, or ASCII whitespace.
- Plugin artifact validation must check both existence and `is_file()`. A directory path must produce a redacted `plugin artifact is not a file` issue.
- Collect all validation issues into one error string beginning with `studio plugin preflight failed:` and one `- ...` line per issue.
- Do not print full local filesystem paths in validation issues. Use labels such as `studio path does not exist`, `plugin artifact does not exist`, and artifact filenames when available.

Render success output exactly in this shape:

```text
PASS studio-plugin-preflight
target: os=<os> arch=<arch> studio_version=<studio-version>
plugin_artifact: <filename>
pandar_commit: <commit>
evidence_row:
| <studio-version> | <os> | <arch> | `<filename>` | `<commit>` | <test-date> | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | Preflight passed only; replace this evidence note after a real Studio run. |
```

Add unit tests in the same file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file(dir: &tempfile::TempDir, name: &str) -> PathBuf {
        let path = dir.path().join(name);
        let mut file = fs::File::create(&path).unwrap();
        writeln!(file, "placeholder").unwrap();
        path
    }

    fn valid_args(dir: &tempfile::TempDir) -> Args {
        Args {
            studio_path: temp_file(dir, "BambuStudio"),
            plugin_artifact: temp_file(dir, "libpandar_network_plugin.so"),
            hub_url: "https://hub.example".to_owned(),
            frontend_url: "https://web.example".to_owned(),
            os: TargetOs::Linux,
            arch: "x86_64".to_owned(),
            studio_version: "1.10.2".to_owned(),
            test_date: "2026-06-24".to_owned(),
            pandar_commit: "abcdef123456".to_owned(),
        }
    }
}
```

Required test cases:

- valid Linux preflight renders `PASS studio-plugin-preflight` and the evidence row;
- Linux target rejects `pandar_network_plugin.dll`;
- Windows target rejects `libpandar_network_plugin.so`;
- macOS target rejects `pandar_network_plugin.dll`;
- missing Studio path and missing plugin artifact are both reported in one failure;
- plugin artifact directory path is rejected with `plugin artifact is not a file`;
- malformed URL scheme is rejected;
- URL credentials such as `https://user:pass@hub.example` are rejected;
- missing required CLI flag is rejected by `parse_args`;
- commit values with `/`, `\`, or whitespace are rejected;
- invalid date shape is rejected;
- evidence row has 17 markdown table cells and 10 `untested` status values.
- valid preflight output includes the explicit `--test-date` value in the evidence row.
- plugin filename/OS mismatch output includes the artifact filename and expected filename, but does not contain the full temporary path.

- [ ] **Step 3: Run focused crate tests**

Run:

```bash
cargo test --manifest-path tools/studio-plugin-smoke/Cargo.toml
```

Expected: all `pandar-studio-plugin-smoke` tests pass.

## Task 2: Document The Preflight Helper

**Files:**
- Modify: `docs/compatibility/bambu-studio-plugin-smoke.md`
- Modify: `docs/compatibility/bambu-studio-plugin.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update the smoke runbook**

In `docs/compatibility/bambu-studio-plugin-smoke.md`, add a `Preflight` section after `Environment` and before `Replace And Roll Back`.

Include the command:

```bash
cargo run --manifest-path tools/studio-plugin-smoke/Cargo.toml -- \
  --preflight \
  --studio-path /path/to/BambuStudio \
  --plugin-artifact /path/to/libpandar_network_plugin.so \
  --hub-url "$PANDAR_PLUGIN_HUB_URL" \
  --frontend-url "$PANDAR_PLUGIN_FRONTEND_URL" \
  --os linux \
  --arch x86_64 \
  --studio-version "1.10.2" \
  --test-date 2026-06-24 \
  --pandar-commit "$(git rev-parse HEAD)"
```

State that a passing preflight only proves prerequisites and metadata shape; the real Studio checklist remains untested until Studio is launched and exercised manually.

- [ ] **Step 2: Update compatibility manifest coverage**

In `docs/compatibility/bambu-studio-plugin.md`, add a row under `Local Automated Probe Coverage`:

```markdown
| `cargo test --manifest-path tools/studio-plugin-smoke/Cargo.toml` plus a valid temporary-file CLI invocation | Validates Phase 23 operator prerequisite shape, plugin filename/OS matching, URL redaction rules, and evidence row formatting before a manual Studio run. | `passed` | 2026-06-24: local helper coverage only; real Studio rows remain blocked until actual Bambu Studio hosts are available. |
```

- [ ] **Step 3: Update roadmap Phase 23**

In `docs/roadmap.md` Phase 23, add a completed/local-scaffolding bullet:

```markdown
- Added a Phase 23 Studio preflight helper that validates local Studio/plugin prerequisite metadata and prints a redacted manifest row template before manual real-Studio testing; it does not claim compatibility without a real Studio run.
```

Keep the existing real Studio blocker and exit criteria unchanged.

## Task 3: Verify, Review, And Prepare For Commit

**Files:**
- Verify all changed files.

- [ ] **Step 1: Run formatting for the standalone crate**

Run:

```bash
cargo fmt --check --manifest-path tools/studio-plugin-smoke/Cargo.toml
```

Expected: exit 0.

- [ ] **Step 2: Run workspace formatting**

Run:

```bash
cargo fmt --check
```

Expected: exit 0.

- [ ] **Step 3: Run clippy for the standalone crate**

Run:

```bash
cargo clippy --manifest-path tools/studio-plugin-smoke/Cargo.toml
```

Expected: exit 0.

- [ ] **Step 4: Run focused tests**

Run:

```bash
cargo test --manifest-path tools/studio-plugin-smoke/Cargo.toml
```

Expected: all tests pass.

- [ ] **Step 5: Run a valid CLI smoke with temporary files**

Run:

```bash
tmp="$(mktemp -d)"
touch "$tmp/BambuStudio" "$tmp/libpandar_network_plugin.so"
cargo run --manifest-path tools/studio-plugin-smoke/Cargo.toml -- \
  --preflight \
  --studio-path "$tmp/BambuStudio" \
  --plugin-artifact "$tmp/libpandar_network_plugin.so" \
  --hub-url https://hub.example \
  --frontend-url https://web.example \
  --os linux \
  --arch x86_64 \
  --studio-version "1.10.2" \
  --test-date 2026-06-24 \
  --pandar-commit abcdef123456
rm -rf "$tmp"
```

Expected: exit 0 and output includes `PASS studio-plugin-preflight`.

- [ ] **Step 6: Run an invalid filename CLI smoke**

Run:

```bash
tmp="$(mktemp -d)"
touch "$tmp/BambuStudio" "$tmp/pandar_network_plugin.dll"
if cargo run --manifest-path tools/studio-plugin-smoke/Cargo.toml -- \
  --preflight \
  --studio-path "$tmp/BambuStudio" \
  --plugin-artifact "$tmp/pandar_network_plugin.dll" \
  --hub-url https://hub.example \
  --frontend-url https://web.example \
  --os linux \
  --arch x86_64 \
  --studio-version "1.10.2" \
  --test-date 2026-06-24 \
  --pandar-commit abcdef123456; then
  echo "expected invalid filename preflight to fail" >&2
  rm -rf "$tmp"
  exit 1
fi
rm -rf "$tmp"
```

Expected: command exits non-zero and reports the filename/OS mismatch.
Also verify the captured output does not contain the full `$tmp` path.

- [ ] **Step 7: Run workspace tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all workspace tests pass.

- [ ] **Step 8: Check diff hygiene**

Run:

```bash
git diff --check
git status --short
```

Expected: no whitespace errors; only the new helper crate, SDD spec/plan, and Phase 23 docs/roadmap files changed.
