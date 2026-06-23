# Phase 23 Real Bambu Studio Plugin Compatibility Design

Phase 23 turns the existing `pandar-network-plugin` scaffold into a compatibility-tested Bambu Studio integration. The implementation must stay focused on evidence, reproducibility, and safer adapter behavior. It must not expand the plugin into a second local agent, add direct MQTT/FTPS access, or claim real Studio compatibility from unit tests alone.

## Current State

- `crates/pandar-network-plugin` already builds a Rust `cdylib` with a C++ ABI shim and exports the required `bambu_network_*` and `ft_*` symbols.
- `crates/pandar-network-plugin/tests/exports.rs` verifies local dynamic-library exports against `docs/superpowers/specs/2026-06-23-phase-21-network-plugin-abi-symbols.txt`.
- `crates/pandar-network-plugin/src/lib.rs` provides FFI-safe HTTP helpers for plugin ticket exchange, printer/job listing, and print submission through `/api/v1/plugin/*`.
- `crates/pandar-network-plugin/src/shim.cpp` stores minimal Studio login state and maps selected ABI calls to the Rust HTTP helpers. Many ABI functions intentionally return stable unsupported or empty values.
- `frontend/app/plugin-sign-in` implements the Studio sign-in page and requests Studio's `get_localhost_url` bridge when available.
- `crates/pandar-hub/src/routes/plugin.rs` and `crates/pandar-hub/src/routes/tests/plugin.rs` cover login-ticket creation/exchange, plugin-scoped token authorization, printer/job listing, plugin print submission, audit metadata, and validation errors.
- `docs/development.md` and `docs/architecture.md` document plugin boundaries and replacement-path guidance, but they still state that real Studio compatibility testing is incomplete.

## Goals

1. Create a repeatable Phase 23 compatibility evidence workflow for real Bambu Studio runs on Linux, Windows, and macOS.
2. Add a repository-native compatibility manifest that records supported Studio versions, OS targets, plugin artifact names, tested ABI behaviors, and unsupported ABI surfaces.
3. Add a local probe harness that exercises the Studio-facing ABI call sequence without requiring Bambu Studio, so regressions in login/profile/printer/job/print adapter behavior are caught in CI.
4. Harden plugin HTTP error mapping so Studio receives useful, stable, redacted failure bodies for invalid hub URL, expired ticket, revoked plugin token, offline hub, bad artifact, unauthorized printer/job access, and unsupported direct-printer/file-transfer paths.
5. Document the manual real-Studio smoke procedure and evidence format so future compatibility claims are tied to captured artifacts, not memory.

## Non-Goals

- No direct printer MQTT, FTPS, SFTP, LAN discovery, or `pandar-agent` sockets inside `pandar-network-plugin`.
- No full Bambu cloud API clone.
- No signing, notarization, installer packaging, or release artifact redesign. Those belong to Phase 24.
- No slicer metadata parsing. That belongs to Phase 28.
- No claim that Phase 23 is complete until at least one real Studio run is recorded in the compatibility manifest. In environments without Bambu Studio, implementation can only complete the local harness and docs milestone.

## Milestones

### Milestone 23.1: Compatibility Manifest And Evidence Schema

Add a checked-in manifest under `docs/compatibility/bambu-studio-plugin.md`.

The manifest records:

- Studio version, OS, architecture, plugin artifact name, Pandar commit, and test date.
- Whether the plugin loaded without missing symbols.
- Whether Studio opened the Pandar sign-in page.
- Whether the Studio localhost callback returned a ticket to the plugin.
- Whether `get_my_token(ticket)` exchanged the ticket successfully.
- Whether `get_my_profile(token)` returned a Bambu-shaped profile accepted by Studio.
- Whether printer listing, job listing, and print submission were attempted and what result occurred.
- Unsupported ABI surfaces that were observed and accepted, such as direct LAN connect or file-transfer tunnel calls.
- Failure evidence with redacted error body, log excerpt, and reproduction notes.

The initial manifest must clearly mark all real Studio platforms as `untested` unless evidence exists in the repository. It must not fabricate compatibility results.

Acceptance criteria:

- The manifest has a stable table/schema for future evidence updates.
- Every compatibility status is one of `passed`, `failed`, `blocked`, `unsupported`, or `untested`.
- The initial file distinguishes local automated probe coverage from real Studio evidence.

### Milestone 23.2: Local Studio ABI Probe Harness

Add a native symbol-level test harness for `crates/pandar-network-plugin` that loads the built `cdylib` and drives the same exported ABI functions Studio is expected to call. The existing `crates/pandar-network-plugin/tests/http_boundary.rs` Rust helper tests remain useful, but they are not sufficient for this milestone because they do not exercise the C++ shim or exported `bambu_network_*` entry points.

1. `bambu_network_create_agent`
2. `bambu_network_get_bambulab_host`
3. `bambu_network_get_my_token`
4. `bambu_network_get_my_profile`
5. `bambu_network_change_user`
6. `bambu_network_build_login_cmd`
7. `bambu_network_build_login_info`
8. `bambu_network_get_user_print_info`
9. `bambu_network_get_user_tasks`
10. `bambu_network_start_print`
11. `bambu_network_user_logout`
12. `bambu_network_build_logout_cmd`
13. `bambu_network_connect_printer`
14. representative `ft_*` tunnel/job functions
15. `bambu_network_destroy_agent`

The harness should use a local mock HTTP server for hub responses and `libloading` or an equivalent dynamic-library loading strategy to call the exported symbols. If a platform cannot safely run the symbol-level probe in CI, the test must skip with an explicit reason while keeping the existing export-list test active. It must verify that:

- the plugin reads `PANDAR_PLUGIN_HUB_URL` and `PANDAR_PLUGIN_FRONTEND_URL`;
- ticket exchange stores token/profile state;
- profile retrieval returns the stored profile;
- login/logout envelope JSON contains the expected Studio command names from `bambu_network_build_login_cmd`, `bambu_network_build_login_info`, and `bambu_network_build_logout_cmd`;
- printer/job listing calls include bearer auth;
- print submission sends artifact bytes as base64 to the hub route;
- failed hub responses are propagated as redacted JSON bodies and non-success ABI return codes;
- unsupported direct-printer and `ft_*` paths return stable unsupported errors without opening machine sockets.

Acceptance criteria:

- `cargo test -p pandar-network-plugin` covers the local ABI probe.
- The probe does not require Bambu Studio, a live printer, external network access, or real credentials.
- The probe does not log or assert plaintext bearer tokens except known fake test tokens.

### Milestone 23.3: Plugin Error Mapping And Redaction

Introduce a small, explicit error mapping boundary inside `pandar-network-plugin` for plugin HTTP and ABI errors. This milestone does not require hub schema, migration, or route changes. The plugin maps the hub's existing HTTP status and JSON error bodies into stable Studio-facing JSON error objects and ABI return codes. If a future hub route later returns a new `410` or `token_revoked` body, the plugin mapping should already handle it, but Phase 23 must not add such a hub route unless the implementation plan explicitly adds corresponding route tests and SQLite/PostgreSQL parity coverage.

Stable plugin error codes:

- `invalid_hub_url`
- `invalid_plugin_ticket`
- `invalid_auth_token`
- `hub_unavailable`
- `plugin_token_revoked`
- `plugin_forbidden`
- `printer_not_found`
- `artifact_missing`
- `artifact_empty`
- `artifact_invalid_base64`
- `artifact_invalid_plate`
- `artifact_too_large`
- `unsupported_direct_printer`
- `unsupported_file_transfer`
- `invalid_response`

Mapping rules:

- HTTP `401` from plugin routes maps to `invalid_auth_token`, except ticket exchange keeps `invalid_plugin_ticket`.
- HTTP `403` maps to `plugin_forbidden`.
- HTTP `404` printer/job route failures map to the hub-provided stable code when present, otherwise `printer_not_found`.
- HTTP `410` or hub response `token_revoked` maps to `plugin_token_revoked` when observed. If the current hub never emits that status/body, a plugin unit test may simulate it at the HTTP boundary.
- Network send failures map to `hub_unavailable`.
- Artifact path read failures map to `artifact_missing` without returning the local path.
- Unsupported direct printer or file-transfer ABI paths return `unsupported_direct_printer` or `unsupported_file_transfer`.
- Response bodies returned to Studio must not contain bearer tokens, plugin tickets, Bambu access codes, local artifact paths, or filesystem paths.

Acceptance criteria:

- Unit tests cover the mapping table for HTTP helper results and ABI update callbacks.
- Existing hub stable error labels are preserved where possible.
- Error bodies remain concise JSON objects with an `error` string and optional redacted `message`.

### Milestone 23.4: Manual Real Studio Smoke Runbook

Add `docs/compatibility/bambu-studio-plugin-smoke.md`.

The runbook must include:

- prerequisite hub, web, external auth, tenant, plugin token, agent, and optional printer setup;
- how to build or select the plugin dynamic library for Linux, Windows, and macOS;
- how to replace the original Studio plugin and how to roll back;
- environment variables `PANDAR_PLUGIN_HUB_URL` and `PANDAR_PLUGIN_FRONTEND_URL`;
- exact smoke checklist for load, sign-in, token/profile, printer list, job list, print submission, logout, and unsupported direct-printer paths;
- evidence capture commands or UI notes;
- redaction instructions for logs and screenshots;
- how to update `docs/compatibility/bambu-studio-plugin.md` after the run.

Acceptance criteria:

- A developer with Bambu Studio installed can follow the runbook without reading source code.
- The runbook avoids requesting Bambu access codes in the plugin path.
- The runbook explicitly states that real compatibility is unproven until the manifest is updated with evidence.

### Milestone 23.5: Documentation And Roadmap Update

Update `docs/development.md`, `docs/architecture.md`, and `docs/roadmap.md` to reflect the Phase 23 state.

Required wording:

- If no real Studio evidence was recorded, say Phase 23 local probe/docs are implemented but real platform compatibility remains unverified.
- If real evidence is recorded, summarize supported Studio version/OS combinations and link to the manifest.
- Keep Phase 24 packaging/signing and release validation separate.

Acceptance criteria:

- Roadmap language does not mark real Studio compatibility complete without manifest evidence.
- Development docs link to the smoke runbook and compatibility manifest.
- Architecture docs still state that the plugin connects only to `pandar-hub`.

## Verification

Required local verification:

```bash
cargo fmt
cargo clippy --workspace
cargo nextest run --manifest-path "Cargo.toml" --workspace
npm --prefix frontend run build
```

Additional focused checks:

```bash
cargo test -p pandar-network-plugin
cargo nextest run -p pandar-hub routes::tests::plugin
```

Manual verification, when a desktop Studio environment is available:

- Run the smoke checklist in `docs/compatibility/bambu-studio-plugin-smoke.md`.
- Update `docs/compatibility/bambu-studio-plugin.md` with the evidence row.

## Risks And Boundaries

- Real Bambu Studio behavior may call ABI functions in a different order than the local probe. The manifest and runbook exist to capture that drift.
- Windows and macOS dynamic-library loading may expose C++ ABI or signing issues that local Linux tests cannot catch.
- Plugin print submission currently reads a local artifact path from Studio and uploads bytes to the hub. Large-artifact transport hardening belongs to Phase 25.
- Direct `ft_*` compatibility remains intentionally unsupported unless future evidence shows Studio requires a richer no-op contract for login or hub-backed print submission.
