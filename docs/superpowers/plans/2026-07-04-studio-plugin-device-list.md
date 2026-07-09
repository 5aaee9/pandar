# Studio Plugin Device List Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Bambu Studio see Pandar printers through the installed networking plugin.

**Architecture:** Keep the network plugin shim as a pass-through and change only the Hub plugin route response contract to Bambu Studio's expected top-level `devices` array with the Studio-native fields consumed by `DeviceManager::parse_user_print_info`. Preserve tenant API behavior and plugin auth checks. Allow the frontend plugin sign-in page to operate in trusted local no-auth development mode so Bambu Studio can acquire the plugin token needed to request that device list.

**Tech Stack:** Rust, axum, serde, cargo nextest.

---

### Task 1: Lock Plugin Printer Response Shape

**Files:**

- Modify: `crates/pandar-hub/src/routes/plugin.rs`
- Modify: `crates/pandar-hub/src/routes/tests/plugin.rs`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Add a failing route test**

Add a test in `crates/pandar-hub/src/routes/tests/plugin.rs` that authenticates as plugin Studio, calls `/api/v1/plugin/printers`, and asserts the body has `devices`, not `printers`. The test should also assert that an `IDLE` printer returns Studio-native fields: `dev_name`, `dev_online: true`, `dev_model_name`, and `task_status`.

- [ ] **Step 2: Update the existing auth-route success assertion**

In `plugin_routes_only_accept_plugin_studio_tokens`, change the accepted plugin response assertion from `{"printers":[]}` to `{"devices":[]}`. Keep the denied-token assertions unchanged.

- [ ] **Step 3: Run the targeted test**

Run:

```powershell
cargo test -p pandar-hub plugin_printer_list_returns_studio_devices_shape
```

Expected before implementation: the test fails because the response contains `printers`.

- [ ] **Step 4: Change the plugin route response**

In `crates/pandar-hub/src/routes/plugin.rs`, rename `PluginPrinterListResponse.printers` to `devices`, add the Studio-native fields consumed by Bambu Studio, and map active statuses such as `IDLE` to `dev_online: true`.

- [ ] **Step 5: Run the targeted tests**

Run:

```powershell
cargo test -p pandar-hub plugin_printer_list_returns_studio_devices_shape
cargo test -p pandar-network-plugin --test studio_abi_probe
```

Expected: both pass.

- [ ] **Step 6: Update roadmap**

Add a concise entry to `docs/roadmap.md` under completed work noting the Bambu Studio plugin printer-list shape fix.

- [ ] **Step 7: Allow local no-auth plugin sign-in**

In `frontend/app/plugin-sign-in/page.tsx`, allow the form path when external auth is disabled but the frontend has no auth provider, no auth token, and tenant lookup succeeds. In `frontend/app/actions.ts`, remove the browser-auth precondition from `createPluginTicket` so Hub remains the authorization boundary.

- [ ] **Step 8: Full verification**

Run:

```powershell
cargo fmt
cargo clippy --workspace
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all commands exit successfully.
