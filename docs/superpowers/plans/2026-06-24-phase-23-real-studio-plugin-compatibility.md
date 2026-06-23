# Phase 23 Real Studio Plugin Compatibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Phase 23 local compatibility evidence, plugin error mapping, ABI probing, and documentation without claiming real Bambu Studio support until manifest evidence exists.

**Architecture:** Keep `pandar-network-plugin` as a hub-only adapter. Rust owns HTTP helper behavior and stable redacted error bodies; the C++ shim owns Studio ABI state and unsupported direct-printer/file-transfer responses. A Rust integration test starts a local mock hub and drives a small C++ probe executable against the built plugin dynamic library so CI exercises the exported C++ ABI without requiring Bambu Studio.

**Tech Stack:** Rust 2024, `reqwest`, `tokio`, C++17 shim compiled by `cc`, Rust integration tests, local `TcpListener` mock HTTP server, Markdown compatibility docs.

---

## Reviewed Inputs

- Spec: `docs/superpowers/specs/2026-06-24-phase-23-real-studio-plugin-compatibility-design.md`
- ABI symbols: `docs/superpowers/specs/2026-06-23-phase-21-network-plugin-abi-symbols.txt`
- Existing plugin HTTP helpers: `crates/pandar-network-plugin/src/lib.rs`
- Existing C++ ABI shim: `crates/pandar-network-plugin/src/shim.cpp`
- Existing plugin tests: `crates/pandar-network-plugin/tests/exports.rs`, `crates/pandar-network-plugin/tests/http_boundary.rs`
- Current docs: `docs/development.md`, `docs/architecture.md`, `docs/roadmap.md`

## Execution Rules

- Implement Phase 23 only.
- Do not commit per task. SDD requires one final Lore-format commit and push only after final implementation review, docs update, and fresh verification. If push is blocked by credentials, network, remote policy, or branch protection, report the local commit SHA and exact push error.
- Do not add direct MQTT, FTPS, SFTP, LAN printer sockets, or `pandar-agent` calls inside `pandar-network-plugin`.
- Do not add hub migrations or persistent routes for Phase 23. If a future task changes hub persistence, it must include SQLite and PostgreSQL parity; this plan does not do that.
- Do not mark real Bambu Studio compatibility complete unless a real Studio evidence row is added to `docs/compatibility/bambu-studio-plugin.md`.
- Keep Phase 24 packaging/signing, Phase 25 upload transport hardening, and Phase 28 slicer metadata parsing out of this implementation.
- For tests that compile or run a C++ probe, skip with a clear message only when the host lacks a usable C++ compiler or platform dynamic loading is not supported; keep the export-list test active.

## File Structure

Create:

- `docs/compatibility/bambu-studio-plugin.md` - compatibility manifest and evidence schema.
- `docs/compatibility/bambu-studio-plugin-smoke.md` - manual real Studio smoke runbook.
- `crates/pandar-network-plugin/tests/studio_abi_probe.rs` - Rust integration test that builds the plugin, starts a local mock hub, compiles/runs the C++ probe, and checks probe output.
- `crates/pandar-network-plugin/tests/fixtures/studio_abi_probe.cpp` - C++17 executable that loads the built plugin and calls the Studio-facing ABI symbols.

Modify:

- `crates/pandar-network-plugin/src/lib.rs` - add explicit stable error mapping/redaction boundary for plugin HTTP helpers.
- `crates/pandar-network-plugin/src/shim.cpp` - return stable unsupported JSON bodies for direct-printer/file-transfer paths and keep print submission display name separate from the artifact path.
- `crates/pandar-network-plugin/tests/http_boundary.rs` - add focused tests for every stable error-code mapping.
- `docs/development.md` - link the compatibility manifest and smoke runbook.
- `docs/architecture.md` - update plugin status from Phase 21 scaffold to Phase 23 local probe/docs when complete, while keeping real Studio compatibility evidence separate.
- `docs/roadmap.md` - record Phase 23 progress and preserve the real-evidence gate.

## Task 1: Compatibility Manifest

**Files:**
- Create: `docs/compatibility/bambu-studio-plugin.md`

- [ ] **Step 1: Add the manifest skeleton**

Create `docs/compatibility/bambu-studio-plugin.md` with this structure:

```markdown
# Bambu Studio Plugin Compatibility

Phase 23 tracks Pandar's Bambu Studio network plugin compatibility evidence. A platform is compatible only after a real Bambu Studio run is recorded here.

## Status Values

| Status | Meaning |
| --- | --- |
| `passed` | Verified in the named environment with evidence captured. |
| `failed` | Attempted and failed; reproduction notes are recorded. |
| `blocked` | Could not complete because of a documented environment or dependency blocker. |
| `unsupported` | Intentionally unsupported by Pandar. |
| `untested` | No evidence has been recorded. |

## Real Studio Evidence

| Studio Version | OS | Arch | Plugin Artifact | Pandar Commit | Test Date | Load | Sign-In Page | Localhost Ticket | Token Exchange | Profile | Printers | Jobs | Print Submission | Logout | Unsupported ABI | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| untested | Linux | x86_64 | `libpandar_network_plugin.so` | none | none | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | No real Studio run recorded. |
| untested | Windows | x86_64 | `pandar_network_plugin.dll` | none | none | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | No real Studio run recorded. |
| untested | macOS | arm64/x86_64 | `libpandar_network_plugin.dylib` | none | none | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | No real Studio run recorded. |

## Local Automated Probe Coverage

| Probe | Coverage | Status | Evidence |
| --- | --- | --- | --- |
| `cargo test -p pandar-network-plugin` | Exported symbol list, Rust HTTP helper boundaries, and local C++ ABI call sequence without Bambu Studio. | `untested` | Update after the implementation lands and tests pass. |

## Unsupported ABI Surfaces

| Surface | Status | Reason |
| --- | --- | --- |
| Direct LAN printer connect/message APIs | `unsupported` | Pandar keeps printer sockets in `pandar-agent`; the plugin talks only to `pandar-hub`. |
| `ft_*` direct file-transfer tunnel/job APIs | `unsupported` | Pandar uploads through hub-backed print submission and does not open direct file-transfer sockets in the plugin. |

## Evidence Requirements

- Record the exact Studio version, OS, architecture, plugin artifact name, Pandar commit, and test date.
- Redact bearer tokens, plugin tickets, Bambu access codes, local artifact paths, and filesystem paths.
- Attach or summarize logs/screenshots only after redaction.
- Keep failed and blocked rows; they are compatibility evidence.
```

- [ ] **Step 2: Verify manifest status vocabulary**

Run:

```bash
rg -n "`(passed|failed|blocked|unsupported|untested)`|`[a-z_]+`" docs/compatibility/bambu-studio-plugin.md
```

Expected: all status cells use only `passed`, `failed`, `blocked`, `unsupported`, or `untested`; non-status code spans are field names or artifact filenames.

## Task 2: Stable Plugin Error Mapping

**Files:**
- Modify: `crates/pandar-network-plugin/src/lib.rs`
- Modify: `crates/pandar-network-plugin/tests/http_boundary.rs`

- [ ] **Step 1: Write failing HTTP boundary tests**

Extend `crates/pandar-network-plugin/tests/http_boundary.rs` with a local one-shot HTTP server helper based on `std::net::TcpListener`. Add tests named:

```rust
#[test]
fn invalid_hub_url_is_rejected_before_network() {}

#[test]
fn syntactically_invalid_hub_url_is_rejected_before_network() {}

#[test]
fn network_failure_maps_to_hub_unavailable() {}

#[test]
fn ticket_exchange_401_maps_to_invalid_plugin_ticket() {}

#[test]
fn empty_auth_token_is_rejected_before_network() {}

#[test]
fn authenticated_401_maps_to_invalid_auth_token() {}

#[test]
fn forbidden_maps_to_plugin_forbidden() {}

#[test]
fn not_found_without_stable_code_maps_to_printer_not_found() {}

#[test]
fn token_revoked_body_maps_to_plugin_token_revoked() {}

#[test]
fn unrecognized_server_error_maps_to_invalid_response() {}

#[test]
fn empty_artifact_is_rejected_before_network() {}

#[test]
fn missing_artifact_is_rejected_without_leaking_path() {}

#[test]
fn hub_artifact_errors_pass_through_when_stable() {}
```

The helper should read one request, assert the expected method/path and optional bearer header, then write a fixed HTTP response. The fake bearer token may be `pandar_plugin_test_token`; do not assert or print real secrets.

- [ ] **Step 2: Run the focused tests and capture the expected failure**

Run:

```bash
cargo test -p pandar-network-plugin --test http_boundary
```

Expected before implementation: one or more new tests fail because `response_result` currently returns raw non-success hub bodies and does not normalize every stable error code.

- [ ] **Step 3: Implement the error mapping boundary**

In `crates/pandar-network-plugin/src/lib.rs`, add a small private boundary:

```rust
#[derive(Clone, Copy)]
enum RequestKind {
    TicketExchange,
    Authenticated,
    PrinterLookup,
    PrintSubmission,
}

fn stable_error_body(error: &str) -> String {
    format!(r#"{{"error":"{error}"}}"#)
}

fn redact_hub_error(kind: RequestKind, http_code: u32, body: &str) -> String {
    let hub_error = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("error").and_then(|error| error.as_str()).map(str::to_owned));
    let error = match (kind, http_code, hub_error.as_deref()) {
        (RequestKind::TicketExchange, 401, _) => "invalid_plugin_ticket",
        (_, 401, _) => "invalid_auth_token",
        (_, 403, _) => "plugin_forbidden",
        (_, 410, _) | (_, _, Some("token_revoked")) => "plugin_token_revoked",
        (RequestKind::PrinterLookup, 404, None) => "printer_not_found",
        (_, _, Some("artifact_invalid_base64")) => "artifact_invalid_base64",
        (_, _, Some("artifact_invalid_plate")) => "artifact_invalid_plate",
        (_, _, Some("artifact_too_large")) => "artifact_too_large",
        (_, _, Some("printer_not_found")) => "printer_not_found",
        (_, _, Some("invalid_plugin_ticket")) => "invalid_plugin_ticket",
        (_, _, Some("invalid_auth_token")) => "invalid_auth_token",
        (_, _, Some("plugin_forbidden")) => "plugin_forbidden",
        _ => "invalid_response",
    };
    stable_error_body(error)
}
```

Wire `post_json` and `get_json` through `RequestKind`, and make `response_result(kind, response)` return:

- success responses unchanged;
- non-success responses with `redact_hub_error(kind, http_code, &body)`;
- unreadable response bodies as `{"error":"invalid_response"}`.

Update `invalid_input` to call `stable_error_body`. Add a `normalize_hub_url` helper that trims trailing slashes and uses `reqwest::Url::parse` to reject empty or syntactically unusable hub URLs before request construction. Reject empty or whitespace-only plugin auth tokens as `invalid_auth_token` before request construction in every authenticated helper. Reject unreadable artifact paths as `artifact_missing` without including the path in the response body, and reject an empty artifact file as `artifact_empty`.

- [ ] **Step 4: Run focused tests**

Run:

```bash
cargo test -p pandar-network-plugin --test http_boundary
```

Expected: all `http_boundary` tests pass.

## Task 3: C++ ABI Probe Harness

**Files:**
- Create: `crates/pandar-network-plugin/tests/fixtures/studio_abi_probe.cpp`
- Create: `crates/pandar-network-plugin/tests/studio_abi_probe.rs`
- Modify: `crates/pandar-network-plugin/src/shim.cpp`

- [ ] **Step 1: Add the C++ probe fixture**

Create `crates/pandar-network-plugin/tests/fixtures/studio_abi_probe.cpp`. It must:

- accept `argv[1]` as the plugin library path and `argv[2]` as a temporary artifact path;
- rely on `PANDAR_PLUGIN_HUB_URL` and `PANDAR_PLUGIN_FRONTEND_URL` from the parent process;
- load the plugin with `dlopen`/`dlsym` on Unix and `LoadLibraryA`/`GetProcAddress` on Windows;
- declare the same minimal `BBL::PrintParams`, `TaskQueryParams`, callbacks, `ft_job_result`, and handle types needed for the tested calls;
- call the spec-listed symbols in order;
- print one final JSON line with fields `ok`, `host`, `login_command`, `login_info`, `logout_command`, `printer_rc`, `tasks_rc`, `print_rc`, `direct_connect_rc`, `direct_message_rc`, `ft_abi_version`, `ft_start_connect_rc`, `ft_sync_rc`, `ft_start_job_rc`, `ft_job_result_ec`, `ft_cancel_rc`, and `update_body`.

The probe must call:

```cpp
auto* agent = bambu_network_create_agent("probe-log");
auto host = bambu_network_get_bambulab_host(agent);
int token_rc = bambu_network_get_my_token(agent, "probe-ticket", &http_code, &http_body);
int profile_rc = bambu_network_get_my_profile(agent, "probe-token", &http_code, &http_body);
int change_rc = bambu_network_change_user(agent, http_body);
auto login = bambu_network_build_login_cmd(agent);
auto login_info = bambu_network_build_login_info(agent);
int printer_rc = bambu_network_get_user_print_info(agent, &http_code, &http_body);
int tasks_rc = bambu_network_get_user_tasks(agent, BBL::TaskQueryParams{}, &tasks_body);
BBL::PrintParams params{};
params.dev_id = "printer-1";
params.task_name = "probe.3mf";
params.filename = artifact_path;
params.plate_index = 1;
params.task_use_ams = true;
int print_rc = bambu_network_start_print(agent, params, update_callback, [] { return false; }, {});
int direct_connect_rc = bambu_network_connect_printer(agent, "printer-1", "127.0.0.1", "user", "password", false);
int direct_message_rc = bambu_network_send_message_to_printer(agent, "printer-1", "{}", 0, 0);
FT_TunnelHandle* tunnel = nullptr;
int ft_version = ft_abi_version();
ft_tunnel_create("{}", &tunnel);
int ft_start_connect_rc = ft_tunnel_start_connect(tunnel, tunnel_connect_callback, nullptr);
int ft_sync_rc = ft_tunnel_sync_connect(tunnel);
FT_JobHandle* job = nullptr;
ft_job_create("{}", &job);
int ft_set_result_rc = ft_job_set_result_cb(job, job_result_callback, nullptr);
int ft_start_job_rc = ft_tunnel_start_job(tunnel, job);
ft_job_result result{};
int ft_get_result_rc = ft_job_get_result(job, 100, &result);
int ft_cancel_rc = ft_job_cancel(job);
ft_job_release(job);
int logout_rc = bambu_network_user_logout(agent, false);
auto logout = bambu_network_build_logout_cmd(agent);
ft_tunnel_shutdown(tunnel);
ft_tunnel_release(tunnel);
bambu_network_destroy_agent(agent);
```

Fail the probe with a non-zero exit code if any success-path call fails, if `host` does not match `PANDAR_PLUGIN_FRONTEND_URL` with a trailing slash, if `login` or `login_info` lacks `studio_userlogin`, if `logout` lacks `studio_useroffline`, or if direct-printer/`ft_*` calls do not return the expected unsupported values. Assert `ft_abi_version() == 1`, `ft_tunnel_start_connect(...) == FT_OK` with an unsupported callback message, `ft_tunnel_sync_connect(...) == FT_EIO`, `ft_tunnel_start_job(...) == FT_OK`, `ft_job_get_result(...) == FT_OK` with result `FT_EIO`, and `ft_job_cancel(...) == FT_OK`.

- [ ] **Step 2: Add the Rust integration test wrapper**

Create `crates/pandar-network-plugin/tests/studio_abi_probe.rs` with helpers copied in style from `exports.rs`:

- `target_dir()` and `dynamic_library_path()`;
- `cargo build -p pandar-network-plugin`;
- a `cxx_compiler()` function that tries `CXX`, then `c++`, `g++`, and `clang++`;
- `compile_probe()` that compiles `tests/fixtures/studio_abi_probe.cpp` into a temporary executable with `-std=c++17`, `-ldl` on Linux, and platform-specific output suffix;
- `spawn_mock_hub()` using `std::net::TcpListener` on `127.0.0.1:0`.

The mock hub must handle these requests and assert bearer auth where expected:

```text
POST /api/v1/plugin/login-tickets/exchange
GET /api/v1/plugin/printers
GET /api/v1/plugin/jobs
POST /api/v1/plugin/prints
```

Responses:

```json
{"token":"probe-token","profile":{"user_id":"user-1","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Probe Tenant"}}
{"printers":[{"id":"printer-1","name":"Probe Printer"}]}
{"jobs":[]}
{"job_id":"job-1","status":"queued"}
```

For the print request, assert the JSON body includes `printer_id = "printer-1"`, `filename = "probe.3mf"` or the artifact basename chosen by the shim, and a base64 artifact equal to the temporary artifact bytes.

- [ ] **Step 3: Add ABI failure-mode coverage**

Extend `crates/pandar-network-plugin/tests/fixtures/studio_abi_probe.cpp` to accept an optional third argument:

```text
success
failure
```

The default is `success`. In `failure` mode, call exported ABI functions against a mock hub that returns failures and assert redacted JSON plus non-success return codes:

```cpp
int ticket_rc = bambu_network_get_my_token(agent, "expired-ticket", &http_code, &http_body);
assert(ticket_rc != 0);
assert(http_code == 401);
assert(http_body.find("\"invalid_plugin_ticket\"") != std::string::npos);

bambu_network_change_user(agent, R"({"token":"probe-token","user_id":"user-1","user_name":"Probe User"})");
int printers_rc = bambu_network_get_user_print_info(agent, &http_code, &http_body);
assert(printers_rc != 0);
assert(http_body.find("\"invalid_auth_token\"") != std::string::npos);

BBL::PrintParams params{};
params.dev_id = "printer-1";
params.task_name = "probe.3mf";
params.filename = artifact_path;
int print_rc = bambu_network_start_print(agent, params, update_callback, [] { return false; }, {});
assert(print_rc != 0);
assert(update_body.find("\"plugin_forbidden\"") != std::string::npos);
```

Add a second Rust test in `studio_abi_probe.rs` named `probe_redacts_failed_hub_responses_through_abi`. Its mock hub must return:

```text
POST /api/v1/plugin/login-tickets/exchange -> 401 {"error":"raw-ticket-message","ticket":"secret"}
GET /api/v1/plugin/printers -> 401 {"error":"raw-auth-message","token":"secret"}
POST /api/v1/plugin/prints -> 403 {"error":"raw-forbidden-message","path":"/tmp/secret.3mf"}
```

Expected: the C++ probe sees only stable errors `invalid_plugin_ticket`, `invalid_auth_token`, and `plugin_forbidden`; output must not contain `secret` or `/tmp/secret.3mf`.

- [ ] **Step 4: Verify shim print display name and artifact path separation**

In `crates/pandar-network-plugin/src/shim.cpp`, keep `params.filename` as the local artifact path passed to `pandar_plugin_submit_print`. Keep the Studio-facing display name separate:

```cpp
const std::string& display_name = params.task_name.empty() ? params.project_name : params.task_name;
const std::string& artifact_path = params.filename;
```

Pass `display_name` as the filename argument and `artifact_path` as the artifact path argument to `pandar_plugin_submit_print`. If the existing shim already has this separation, leave it unchanged and let the ABI probe verify it.

- [ ] **Step 5: Return stable unsupported ABI bodies**

In `crates/pandar-network-plugin/src/shim.cpp`, make direct-printer and file-transfer unsupported paths expose the stable error strings from the spec:

```cpp
R"({"error":"unsupported_direct_printer"})"
R"({"error":"unsupported_file_transfer"})"
```

For functions that only return an integer and have no body callback, keep a non-success return code and ensure the ABI probe asserts only the code. For `bambu_network_start_send_gcode_to_sdcard` and `ft_tunnel_start_connect`, update callback messages to the stable JSON body.

- [ ] **Step 6: Run the ABI probe**

Run:

```bash
cargo test -p pandar-network-plugin --test studio_abi_probe -- --nocapture
```

Expected: the test builds the plugin, compiles the C++ probe, runs it against the local mock hub, and passes. If no C++ compiler is available, the test prints a skip reason and returns `Ok(())` without failing.

## Task 4: Manual Smoke Runbook

**Files:**
- Create: `docs/compatibility/bambu-studio-plugin-smoke.md`

- [ ] **Step 1: Add the runbook**

Create `docs/compatibility/bambu-studio-plugin-smoke.md` with sections:

````markdown
# Bambu Studio Plugin Smoke Runbook

## Scope

This runbook records real Bambu Studio compatibility evidence for Phase 23. A successful local ABI probe is not a real Studio compatibility claim.

## Prerequisites

- A running `pandar-hub` reachable from the desktop host.
- A running `pandar-web` with external auth configured.
- A tenant with at least one user who can create plugin login tickets.
- A linked `pandar-agent`.
- Optional: a real printer connected through the agent for print submission.

## Build Or Select Plugin Artifact

Linux:
```bash
cargo build -p pandar-network-plugin --release
```

Windows and macOS release artifacts should come from Phase 24 release validation when available; for Phase 23 manual testing, record the exact artifact path and commit used.

## Environment

```bash
export PANDAR_PLUGIN_HUB_URL="https://your-hub.example"
export PANDAR_PLUGIN_FRONTEND_URL="https://your-web.example"
```

## Replace And Roll Back

1. Locate the original Bambu Studio network plugin dynamic library.
2. Copy it to a timestamped backup path.
3. Replace it with the Pandar plugin artifact for the same OS/architecture.
4. To roll back, quit Studio and restore the backup file.

## Smoke Checklist

| Step | Expected Result | Status | Evidence |
| --- | --- | --- | --- |
| Studio starts and loads plugin | No missing-symbol or dynamic-loader error. | `untested` | |
| Login opens Pandar sign-in | Studio WebView displays Pandar sign-in. | `untested` | |
| Localhost ticket callback completes | Studio receives plugin ticket through its local callback. | `untested` | |
| Token/profile exchange completes | Studio receives Bambu-shaped login state. | `untested` | |
| Printer list loads | Hub-backed printers display or an empty list is accepted. | `untested` | |
| Job list loads | Hub-backed jobs display or an empty list is accepted. | `untested` | |
| Print submission | Optional print submits through `/api/v1/plugin/prints`. | `untested` | |
| Logout | Studio receives `studio_useroffline`. | `untested` | |
| Direct-printer/`ft_*` paths | Unsupported behavior is stable and does not open machine sockets. | `untested` | |

## Evidence Capture And Redaction

- Capture Studio version, OS, architecture, artifact name, Pandar commit, and test date.
- Redact bearer tokens, plugin tickets, Bambu access codes, local artifact paths, and filesystem paths.
- Prefer short log excerpts over full logs.

## Updating The Manifest

After the run, update `docs/compatibility/bambu-studio-plugin.md` with one row per Studio version/OS/architecture and keep failed or blocked attempts.
````

- [ ] **Step 2: Verify runbook links and status language**

Run:

```bash
rg -n "untested|passed|failed|blocked|unsupported|PANDAR_PLUGIN_" docs/compatibility/bambu-studio-plugin-smoke.md
```

Expected: status vocabulary and required environment variables are present.

## Task 5: Documentation And Roadmap Updates

**Files:**
- Modify: `docs/development.md`
- Modify: `docs/architecture.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update development docs**

In `docs/development.md` under "Bambu Studio Network Plugin", replace the final Phase 21 sentence with wording that distinguishes local Phase 23 coverage from real Studio evidence:

```markdown
Phase 23 adds a compatibility manifest, manual smoke runbook, stable plugin error mapping, and a local ABI probe. Real Bambu Studio compatibility remains unverified until `docs/compatibility/bambu-studio-plugin.md` contains a real Studio evidence row.
```

Add links to:

- `docs/compatibility/bambu-studio-plugin.md`
- `docs/compatibility/bambu-studio-plugin-smoke.md`

- [ ] **Step 2: Update architecture docs**

In `docs/architecture.md` under `pandar-network-plugin`, update the Phase 21 scaffold bullet to:

```markdown
- Phase 23 local probe/docs coverage exercises the exported C++ ABI against a mock hub and documents the real Studio smoke process; real platform compatibility is still gated by manifest evidence.
```

Keep the bullet that says the plugin connects only to `pandar-hub`.

- [ ] **Step 3: Update roadmap**

In `docs/roadmap.md` under Phase 23, add completed bullets only for local work:

```markdown
- Completed local Phase 23 scaffolding: compatibility manifest, smoke runbook, stable plugin error mapping, and a local C++ ABI probe against a mock hub.
- Real Studio compatibility remains unverified until `docs/compatibility/bambu-studio-plugin.md` records real Studio runs for each platform.
```

Do not mark the Phase 23 exit criteria complete unless real Studio evidence exists.

- [ ] **Step 4: Verify docs references**

Run:

```bash
rg -n "bambu-studio-plugin|smoke|real Studio compatibility remains|Phase 23" docs/development.md docs/architecture.md docs/roadmap.md docs/compatibility
```

Expected: all new docs are linked and the real-evidence caveat appears in development, architecture, roadmap, and manifest/runbook docs.

## Task 6: Final Verification

**Files:**
- All files touched by Tasks 1-5.

- [ ] **Step 1: Format**

Run:

```bash
cargo fmt
```

Expected: exits 0.

- [ ] **Step 2: Focused plugin tests**

Run:

```bash
cargo test -p pandar-network-plugin
```

Expected: export-list tests, HTTP boundary tests, and local ABI probe pass or the ABI probe explicitly skips only for a missing C++ compiler/platform loader.

- [ ] **Step 3: Hub plugin route regression**

Run:

```bash
cargo nextest run -p pandar-hub routes::tests::plugin
```

Expected: existing plugin route tests pass.

- [ ] **Step 4: Workspace lint and tests**

Run:

```bash
cargo clippy --workspace
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: both exit 0.

- [ ] **Step 5: Frontend build**

Run:

```bash
npm --prefix frontend run build
```

Expected: exits 0.

- [ ] **Step 6: Review final diff before implementation review**

Run:

```bash
git status --short
git diff --stat
git diff --check
```

Expected: only Phase 23 files changed; `git diff --check` exits 0.
