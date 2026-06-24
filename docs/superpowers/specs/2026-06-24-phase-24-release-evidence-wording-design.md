# Phase 24 Release Evidence Wording Design

## Goal

Keep Phase 24 documentation aligned with the current evidence state: workflow artifacts exist for five target families from `release.yml` run `28102001464`, but no tagged GitHub Release, git tag, Windows arm64 passing artifact, or real host installation evidence exists yet.

## Scope

This is a documentation-only correction. It updates Phase 24 status language in:

- `docs/release-installation.md`
- `docs/compatibility/release-artifacts.md`
- `docs/roadmap.md`

No release workflow, packaging code, smoke helper, or product behavior changes are in scope.

## Current Evidence

- `gh release list --limit 20` returned no GitHub Releases on 2026-06-24.
- `git tag --sort=-creatordate | head -20` returned no tags on 2026-06-24.
- `gh api repos/5aaee9/pandar/actions/runs/28102001464/artifacts --jq '.artifacts[] | [.name, .expired, .size_in_bytes] | @tsv'` showed unexpired uploaded artifacts for `linux-amd64`, `linux-arm64`, `windows-amd64`, `macos-amd64`, and `macos-arm64`.
- The same run built, packaged, and checksum-verified Windows arm64 before failing release-smoke plugin export inspection. Because `.github/workflows/release.yml` uploads artifacts only after release-smoke, no Windows arm64 artifact was uploaded for run `28102001464`.
- `release.yml` run `28103772270` did not start build steps because GitHub Actions billing or spending-limit state blocked the run.
- Real host installation status remains `untested` for all target families.

## Required Documentation Behavior

- Distinguish workflow artifact evidence from tagged GitHub Release evidence.
- Keep operator status blocked when real host installation is untested.
- Do not say no release artifacts exist for targets that have workflow artifacts from run `28102001464`.
- Keep Windows arm64 separate because the latest passing artifact evidence is still blocked.
- Keep the roadmap next action focused on selecting a tagged GitHub Release archive or suitable workflow artifact, then running real host installation validation.
- Preserve existing per-run evidence rows. They record historical run outcomes and should not be rewritten during this wording pass.
- Preserve failed and blocked generic rows, but reword them so:
  - The five target families with uploaded run `28102001464` artifacts say no tagged GitHub Release or real host installation evidence exists yet, and point readers to the workflow-run evidence above.
  - Windows arm64 says no tagged GitHub Release, no uploaded passing workflow artifact, and no real host installation evidence exists yet; its build/package/checksum evidence and release-smoke failure are tracked in the per-run row above.

## Acceptance Criteria

- `docs/release-installation.md` target rows accurately describe:
  - Five target families have workflow artifact evidence but no tagged GitHub Release or real host install evidence.
  - Windows arm64 was built and packaged in run `28102001464`, but release-smoke failed before upload; after the LLVM inspector fix, the follow-up workflow run was billing-blocked.
  - Next actions for the five target families with uploaded workflow artifacts allow selecting a tagged GitHub Release archive or suitable workflow artifact before real host installation validation.
  - Windows arm64 next action requires a successful rerun or tagged release artifact before real host installation validation.
- `docs/compatibility/release-artifacts.md` keeps the generic blocked rows and rewords them conditionally:
  - Five target families with uploaded workflow artifacts say no tagged GitHub Release or real host installation evidence exists yet, while workflow-run artifact evidence is tracked above.
  - Windows arm64 says no tagged GitHub Release, no uploaded passing workflow artifact, and no real host installation evidence exists yet.
- `docs/compatibility/release-artifacts.md` keeps the first availability-check row as historical pre-workflow evidence by labeling it as the initial check, then adds a current availability check directly after that initial row. The current row records no GitHub Releases or tags, uploaded run `28102001464` artifacts for five target families, no uploaded Windows arm64 artifact, and the billing-blocked follow-up run.
- `docs/roadmap.md` rewords the Phase 24 availability bullet so the no-workflow-runs statement is framed as the initial pre-workflow availability state, not the current state.
- Verification is limited to documentation hygiene and the unchanged Phase 24 smoke helper:
  - `cargo fmt --check`
  - `git diff --check`
  - `cargo test --manifest-path tools/release-smoke/Cargo.toml`
  - The docs-only scope intentionally does not rerun `cargo clippy` or workspace `cargo nextest`; no Rust code, build script, workflow, or package manifest changes are made.
