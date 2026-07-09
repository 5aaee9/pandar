# Studio Plugin Camera Check Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the Bambu Studio networking plugin runtime check by fixing the Studio camera failure caused by an unresolvable FFmpeg executable.

**Architecture:** Keep Bambu Studio talking to the network plugin ABI, the plugin returning Hub camera URLs, Hub opening reverse camera streams, and Agent using FFmpeg to transcode the printer RTSPS feed. Only make FFmpeg executable lookup explicit enough for local development and diagnostics.

**Tech Stack:** Rust, axum, tonic, FFmpeg, Bambu Studio network plugin ABI.

---

### Task 1: Add explicit FFmpeg executable lookup

**Files:**

- Modify: `crates/pandar-agent/src/machine/camera.rs`

- [ ] Add a helper that returns `PANDAR_FFMPEG_PATH` when set and non-empty after trimming whitespace, and otherwise returns `ffmpeg`.
- [ ] Use that helper in both MJPEG and fragmented MP4 FFmpeg command builders.
- [ ] Add unit coverage for unset, whitespace-only, and explicit override behavior through a pure helper that accepts the env value; do not mutate process environment in tests.
- [ ] Add a diagnostic test that an invalid explicit FFmpeg executable path preserves the attempted executable path and the lower spawn error context in `{err:#}` output.

### Task 2: Preserve Studio light and camera plugin behavior

**Files:**

- Modify: `crates/pandar-network-plugin/src/gcode.rs`
- Add: `crates/pandar-network-plugin/tests/operation_parser.rs`
- Modify: `crates/pandar-network-plugin/tests/http_boundary.rs` only to move operation parser tests out if needed.

- [ ] Keep Studio `system.ledctrl` parsing mapped to `set_chamber_light`.
- [ ] Keep `toggle_light` and `set_chamber_light` accepted at the plugin boundary.
- [ ] Move operation parser coverage out of the already oversized `http_boundary.rs` into a focused test file before adding more parser assertions.
- [ ] Verify `cargo test -p pandar-network-plugin`.

### Task 3: Runtime check in Bambu Studio

**Files:**

- Runtime only.

- [ ] Install `target/debug/pandar_network_plugin.dll` with `pandar install-network-plugin --plugin-file`.
- [ ] Restart Bambu Studio and confirm the plugin DLL timestamp is not overwritten.
- [ ] Login through the local plugin callback.
- [ ] Restore the local printer display name to `X2D` through the existing printer update API if the dev database still contains the serial as name.
- [ ] Restart Agent with `PANDAR_FFMPEG_PATH` pointing at the installed FFmpeg executable.
- [ ] Select the printer in Studio and confirm controls enable.
- [ ] Click Lamp and confirm Studio remains connected.
- [ ] Click Camera play and confirm the route returns media bytes and Studio does not immediately show `Failed to connect to the printer`.

### Task 4: Documentation and final verification

**Files:**

- Modify: `docs/roadmap.md`

- [ ] Add a completed roadmap entry for the Studio plugin runtime check and FFmpeg camera fix.
- [ ] Run:

```powershell
cargo fmt
cargo clippy --workspace
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

- [ ] Review the final diff and commit with Conventional Commits.
