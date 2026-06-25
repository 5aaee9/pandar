# Phase 24 Release Evidence Wording Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Correct Phase 24 documentation so workflow artifact evidence, tagged GitHub Release evidence, and real host installation evidence are not conflated.

**Architecture:** Documentation remains the source of truth for Phase 24 status. The change is a narrow wording update across the operator installation guide, release artifact evidence manifest, and roadmap.

**Tech Stack:** Markdown documentation, existing Rust formatting check, and the existing `tools/release-smoke` test suite.

---

### Task 1: Update Phase 24 Release Evidence Wording

**Files:**

- Modify: `docs/release-installation.md`
- Modify: `docs/compatibility/release-artifacts.md`
- Modify: `docs/roadmap.md`

- [x] **Step 1: Update operator target status**

  In `docs/release-installation.md`, revise the unsupported or untested targets table so each target keeps `blocked` operator status but uses current evidence:
  - `linux-amd64`, `linux-arm64`, `windows-amd64`, `macos-amd64`, and `macos-arm64`: workflow artifact evidence exists from run `28102001464`, but no tagged GitHub Release archive or real host installation evidence exists.
  - `windows-arm64`: run `28102001464` built, packaged, and checksum-verified the target but release-smoke failed before `upload-artifact`; no passing uploaded artifact exists after the LLVM inspector fix because run `28103772270` was blocked before build steps.

  For the Next action column, use "select a tagged GitHub Release archive or suitable workflow artifact" for the five target families with uploaded workflow artifacts. For Windows arm64, require a successful rerun or tagged release artifact before real host installation validation.

- [x] **Step 2: Update release artifact manifest blocked rows**

  In `docs/compatibility/release-artifacts.md`, keep the generic `blocked` rows and revise them so they describe missing tagged GitHub Release and real host install evidence, not missing workflow artifacts for every target.

  For `linux-amd64`, `linux-arm64`, `windows-amd64`, `macos-amd64`, and `macos-arm64`, explicitly point readers to the uploaded workflow-run evidence above. For `windows-arm64`, say no tagged GitHub Release, no uploaded passing workflow artifact, and no real host installation evidence exists yet; its build/package/checksum evidence and release-smoke failure are tracked in the per-run row above.

- [x] **Step 3: Add current release availability check**

  In `docs/compatibility/release-artifacts.md`, relabel the existing initial availability row as the pre-workflow initial check, then add a current 2026-06-24 availability row directly after that initial row. The current row records:
  - No GitHub Releases.
  - No git tags.
  - Unexpired workflow artifacts from run `28102001464` for five target families.
  - No uploaded Windows arm64 artifact from run `28102001464`, because the workflow uploads artifacts only after release-smoke and Windows arm64 failed release-smoke.
  - Run `28103772270` remains blocked by GitHub Actions billing or spending-limit state.

  Do not rewrite the existing per-run evidence rows for runs `28098334876`, `28099917011`, `28102001464`, or `28103772270`.

- [x] **Step 4: Update roadmap Phase 24 and Immediate Next wording**

  In `docs/roadmap.md`, replace the stale Phase 24 bullet that says no `release.yml` workflow runs exist with wording that captures it as the initial pre-workflow availability state before later workflow evidence. Update the Immediate Next Phase 24 line to mention selecting a tagged GitHub Release archive or suitable workflow artifact before real host install validation.

- [x] **Step 5: Verify documentation and smoke helper**

  Run:

  ```bash
  cargo fmt --check
  git diff --check
  cargo test --manifest-path tools/release-smoke/Cargo.toml
  ```

  Expected result: all commands exit 0. `cargo test --manifest-path tools/release-smoke/Cargo.toml` should report the existing release-smoke unit tests passing.

  Do not run full `cargo clippy` or workspace `cargo nextest` for this docs-only milestone unless code, workflow, build script, or package manifest files change.
