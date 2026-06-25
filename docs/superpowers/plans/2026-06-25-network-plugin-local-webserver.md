# Network Plugin Local Webserver Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a loopback-only `pandar-network-plugin` sign-in webserver with an embedded static target-server page and route Studio sign-in through it.

**Architecture:** Add a small static `frontend/plugin-local` workspace package whose committed `dist` output is embedded into `pandar-network-plugin` with `rust-embed`. The Rust plugin owns one `OnceLock` loopback HTTP server with `Arc<Mutex<LocalWebserverConfig>>`, and the C++ shim starts it from `bambu_network_get_bambulab_host` while refreshing selected `hub_url` before hub-facing ABI calls.

**Tech Stack:** Rust 2024, `rust-embed 8.11.0`, existing C++17 shim, TypeScript static page, npm workspaces, existing cargo/nextest verification.

---

## File Structure

- Modify `Cargo.toml`: add `rust-embed = "8.11.0"` to workspace dependencies.
- Modify `crates/pandar-network-plugin/Cargo.toml`: add `rust-embed.workspace = true`.
- Modify `crates/pandar-network-plugin/src/lib.rs`: declare the local webserver module and keep only exported ABI wrappers/public wiring here.
- Create `crates/pandar-network-plugin/src/local_webserver.rs`: own route handling, config normalization, request parsing, `rust-embed` asset lookup, and focused unit helpers.
- Modify `crates/pandar-network-plugin/src/shim.cpp`: add only the required Rust FFI declarations, target/default tracking fields, local server startup helper, config refresh helper, and call sites. Keep new helper logic compact because the existing ABI shim is already centralized in this file.
- Modify `crates/pandar-network-plugin/tests/http_boundary.rs`: add one serial local webserver lifecycle test.
- Modify `crates/pandar-network-plugin/tests/fixtures/studio_abi_probe.cpp`: expect loopback host and verify env precedence through the probe environment.
- Modify `crates/pandar-network-plugin/tests/studio_abi_probe.rs`: set frontend/hub env vars for the probe and adapt assertions.
- Modify `frontend/package.json`: add npm workspace `plugin-local`.
- Create `frontend/plugin-local/package.json`: static package metadata and build script.
- Create `frontend/plugin-local/tsconfig.json`: compile TypeScript to `dist/assets`.
- Create `frontend/plugin-local/scripts/build.mjs`: run `tsc`, copy HTML/CSS to committed `dist`.
- Create `frontend/plugin-local/src/index.html`, `src/styles.css`, `src/app.ts`: product UI target server page.
- Create committed `frontend/plugin-local/dist/index.html`, `dist/assets/styles.css`, `dist/assets/app.js`: embedded runtime assets built from source.
- Modify `docs/architecture.md`, `docs/development.md`, `docs/compatibility/bambu-studio-plugin.md`, `docs/roadmap.md`: document local loopback sign-in, env vars, no remote fallback, and remaining real Studio evidence gap.

## Task 1: Static Plugin-Local Frontend Package

**Files:**

- Modify: `frontend/package.json`
- Create: `frontend/plugin-local/package.json`
- Create: `frontend/plugin-local/tsconfig.json`
- Create: `frontend/plugin-local/scripts/build.mjs`
- Create: `frontend/plugin-local/src/index.html`
- Create: `frontend/plugin-local/src/styles.css`
- Create: `frontend/plugin-local/src/app.ts`
- Create: `frontend/plugin-local/dist/index.html`
- Create: `frontend/plugin-local/dist/assets/styles.css`
- Create: `frontend/plugin-local/dist/assets/app.js`

- [ ] **Step 1: Add npm workspace**

In `frontend/package.json`, add a root workspace while keeping the existing Next.js package at the root:

```json
"workspaces": [
  "plugin-local"
]
```

- [ ] **Step 2: Create plugin-local package metadata**

Create `frontend/plugin-local/package.json`:

```json
{
  "name": "pandar-plugin-local",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "build": "node scripts/build.mjs"
  }
}
```

- [ ] **Step 3: Create TypeScript config**

Create `frontend/plugin-local/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["DOM", "ES2022"],
    "module": "ES2022",
    "moduleResolution": "bundler",
    "strict": true,
    "outDir": "dist/assets",
    "rootDir": "src",
    "noEmitOnError": true
  },
  "include": ["src/app.ts"]
}
```

- [ ] **Step 4: Create build script**

Create `frontend/plugin-local/scripts/build.mjs`:

```js
import { copyFile, mkdir, rm } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const dist = join(root, "dist");

await rm(dist, { force: true, recursive: true });
await mkdir(join(dist, "assets"), { recursive: true });

const tsc = spawnSync("npx", ["tsc", "-p", "tsconfig.json"], {
  cwd: root,
  stdio: "inherit",
  shell: process.platform === "win32",
});
if (tsc.status !== 0) {
  process.exit(tsc.status ?? 1);
}

await copyFile(join(root, "src", "index.html"), join(dist, "index.html"));
await copyFile(
  join(root, "src", "styles.css"),
  join(dist, "assets", "styles.css"),
);
```

- [ ] **Step 5: Create HTML shell**

Create `frontend/plugin-local/src/index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Pandar Studio Sign-in</title>
    <link rel="stylesheet" href="/assets/styles.css" />
  </head>
  <body>
    <main class="shell">
      <section class="panel" aria-labelledby="title">
        <header class="header">
          <div>
            <h1 id="title">Pandar Studio sign-in</h1>
            <p>Choose the Pandar target used by this Studio plugin session.</p>
          </div>
          <span id="status" class="status">Loading</span>
        </header>

        <div id="defaultNotice" class="notice" hidden></div>

        <form id="targetForm" class="form">
          <label>
            <span>Pandar web server</span>
            <input id="webUrl" name="webUrl" autocomplete="url" required />
          </label>
          <label>
            <span>Pandar API server</span>
            <input id="hubUrl" name="hubUrl" autocomplete="url" required />
          </label>
          <div id="error" class="error" role="alert" hidden></div>
          <button type="submit">Switch Target server</button>
        </form>

        <div class="actions">
          <a id="continueLink" class="primary" href="#" aria-disabled="true"
            >Continue sign-in</a
          >
        </div>
      </section>
    </main>
    <script type="module" src="/assets/app.js"></script>
  </body>
</html>
```

- [ ] **Step 6: Create restrained product styling**

Create `frontend/plugin-local/src/styles.css` with compact, accessible product UI:

```css
* {
  box-sizing: border-box;
}

html {
  background: #f1f5f9;
  color: #0f172a;
  font-family: Arial, Helvetica, sans-serif;
}

body {
  margin: 0;
}

.shell {
  min-height: 100vh;
  display: grid;
  place-items: start center;
  padding: 24px 16px;
}

.panel {
  width: min(100%, 680px);
  border: 1px solid #cbd5e1;
  border-radius: 8px;
  background: #ffffff;
}

.header {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  border-bottom: 1px solid #e2e8f0;
  padding: 16px;
}

h1 {
  margin: 0;
  font-size: 1.125rem;
  line-height: 1.4;
}

p {
  margin: 4px 0 0;
  color: #475569;
  font-size: 0.875rem;
  line-height: 1.5;
}

.status {
  align-self: start;
  border-radius: 4px;
  background: #334155;
  color: #ffffff;
  font-size: 0.75rem;
  font-weight: 700;
  padding: 4px 8px;
}

.notice {
  margin: 16px 16px 0;
  border: 1px solid #f59e0b;
  border-radius: 6px;
  background: #fffbeb;
  color: #78350f;
  padding: 12px;
  font-size: 0.875rem;
  line-height: 1.5;
}

.form {
  display: grid;
  gap: 12px;
  padding: 16px;
}

label {
  display: grid;
  gap: 6px;
  font-size: 0.875rem;
  font-weight: 700;
}

input {
  min-height: 40px;
  border: 1px solid #94a3b8;
  border-radius: 6px;
  padding: 8px 10px;
  color: #0f172a;
  font: inherit;
}

input:focus,
button:focus,
.primary:focus {
  outline: 3px solid #67e8f9;
  outline-offset: 2px;
}

button,
.primary {
  min-height: 40px;
  border: 0;
  border-radius: 6px;
  background: #0e7490;
  color: #ffffff;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 0.875rem;
  font-weight: 700;
  padding: 0 12px;
  text-decoration: none;
}

button:hover,
.primary:hover {
  background: #155e75;
}

.primary[aria-disabled="true"] {
  background: #64748b;
  pointer-events: none;
}

.actions {
  border-top: 1px solid #e2e8f0;
  padding: 16px;
}

.error {
  border: 1px solid #b91c1c;
  border-radius: 6px;
  background: #fef2f2;
  color: #7f1d1d;
  padding: 10px;
  font-size: 0.875rem;
}

@media (max-width: 520px) {
  .header {
    flex-direction: column;
  }
}
```

- [ ] **Step 7: Create browser logic**

Create `frontend/plugin-local/src/app.ts`:

```ts
type Config = {
  webUrl: string;
  hubUrl: string;
  usingDefaultServer: boolean;
  usingDefaultWebServer: boolean;
  usingDefaultHubServer: boolean;
  callbackUrl: string;
};

const statusEl = document.querySelector<HTMLSpanElement>("#status")!;
const noticeEl = document.querySelector<HTMLDivElement>("#defaultNotice")!;
const errorEl = document.querySelector<HTMLDivElement>("#error")!;
const webInput = document.querySelector<HTMLInputElement>("#webUrl")!;
const hubInput = document.querySelector<HTMLInputElement>("#hubUrl")!;
const continueLink =
  document.querySelector<HTMLAnchorElement>("#continueLink")!;
const form = document.querySelector<HTMLFormElement>("#targetForm")!;

function setError(message: string | null) {
  errorEl.hidden = message === null;
  errorEl.textContent = message ?? "";
}

function applyConfig(config: Config) {
  webInput.value = config.webUrl;
  hubInput.value = config.hubUrl;
  statusEl.textContent = config.usingDefaultServer
    ? "Default target"
    : "Configured target";
  noticeEl.hidden = !config.usingDefaultServer;
  if (config.usingDefaultServer) {
    const defaults = [
      config.usingDefaultWebServer ? "web server" : null,
      config.usingDefaultHubServer ? "API server" : null,
    ]
      .filter(Boolean)
      .join(" and ");
    noticeEl.textContent = `No explicit ${defaults} configuration was found. Confirm these defaults or switch to your Pandar deployment.`;
  }
  const signIn = new URL("/plugin-sign-in", config.webUrl);
  signIn.searchParams.set("redirect_url", config.callbackUrl);
  continueLink.href = signIn.toString();
  continueLink.setAttribute("aria-disabled", "false");
}

async function loadConfig() {
  setError(null);
  const response = await fetch("/config", { cache: "no-store" });
  if (!response.ok) {
    throw new Error(`Config lookup returned ${response.status}`);
  }
  applyConfig((await response.json()) as Config);
}

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  setError(null);
  const response = await fetch("/config", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      webUrl: webInput.value,
      hubUrl: hubInput.value,
    }),
  });
  if (!response.ok) {
    setError("Enter valid http:// or https:// target server URLs.");
    return;
  }
  applyConfig((await response.json()) as Config);
});

loadConfig().catch((error) => {
  statusEl.textContent = "Unavailable";
  setError(error instanceof Error ? error.message : "Config lookup failed");
});
```

- [ ] **Step 8: Build and commit generated static assets**

Run:

```bash
npm install --prefix frontend
npm run build -w pandar-plugin-local --prefix frontend
```

Expected: `frontend/plugin-local/dist/index.html`, `frontend/plugin-local/dist/assets/app.js`, and `frontend/plugin-local/dist/assets/styles.css` exist and are not ignored by git.

## Task 2: Rust Embedded Local Webserver

**Files:**

- Modify: `Cargo.toml`
- Modify: `crates/pandar-network-plugin/Cargo.toml`
- Modify: `crates/pandar-network-plugin/src/lib.rs`
- Create: `crates/pandar-network-plugin/src/local_webserver.rs`
- Modify: `crates/pandar-network-plugin/tests/http_boundary.rs`

- [ ] **Step 1: Add dependency**

Add to root `Cargo.toml` workspace dependencies:

```toml
rust-embed = "8.11.0"
```

Add to `crates/pandar-network-plugin/Cargo.toml` dependencies:

```toml
rust-embed.workspace = true
```

- [ ] **Step 2: Add local webserver module wiring**

In `crates/pandar-network-plugin/src/lib.rs`, add:

```rust
mod local_webserver;
```

Keep existing HTTP helper code in `lib.rs` unchanged except for calling into the module from the exported wrappers.

- [ ] **Step 3: Create local webserver module**

Create `crates/pandar-network-plugin/src/local_webserver.rs` and put server primitives and embedded assets there:

```rust
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex, OnceLock},
    thread,
};

use rust_embed::RustEmbed;
use serde_json::json;

use crate::{invalid_input, result, stable_error_body, PluginHttpResult};

#[derive(RustEmbed)]
#[folder = "../../frontend/plugin-local/dist/"]
struct PluginLocalAssets;

static LOCAL_WEBSERVER: OnceLock<LocalWebserver> = OnceLock::new();

const MAX_LOCAL_HEADERS: usize = 16 * 1024;
const MAX_LOCAL_CONFIG_BODY: usize = 8 * 1024;
```

- [ ] **Step 4: Add config and server structs**

Add focused structs in `local_webserver.rs`:

```rust
#[derive(Clone)]
struct LocalWebserverConfig {
    web_url: String,
    hub_url: String,
    using_default_web_server: bool,
    using_default_hub_server: bool,
    user_selected: bool,
}

struct LocalWebserver {
    base_url: String,
    config: Arc<Mutex<LocalWebserverConfig>>,
}

struct LocalRequest {
    method: String,
    path: String,
    body: String,
}
```

- [ ] **Step 5: Add module public functions and lib exports**

In `local_webserver.rs`, expose safe Rust functions:

```rust
pub fn start(web_url: String, hub_url: String, web_configured: bool, hub_configured: bool) -> PluginHttpResult
pub fn base_url() -> PluginHttpResult
pub fn config() -> PluginHttpResult
```

In `lib.rs`, add only the C ABI wrappers:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_start_local_webserver(
    web_url_ptr: *const u8,
    web_url_len: usize,
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    web_configured: bool,
    hub_configured: bool,
) -> PluginHttpResult {
    let Some(web_url) = read_utf8(web_url_ptr, web_url_len) else {
        return invalid_input("invalid_target_server");
    };
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len) else {
        return invalid_input("invalid_target_server");
    };
    local_webserver::start(web_url, hub_url, web_configured, hub_configured)
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_local_webserver_base_url() -> PluginHttpResult {
    local_webserver::base_url()
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_local_webserver_config() -> PluginHttpResult {
    local_webserver::config()
}
```

- [ ] **Step 6: Implement singleton startup and config JSON**

In `local_webserver.rs`, add helpers that use `OnceLock::get_or_init`, update only when `user_selected` is false, and return snake_case JSON for ABI helpers.

- [ ] **Step 7: Implement blocking HTTP request parser**

Add `read_local_request(stream: &mut TcpStream) -> std::io::Result<Option<LocalRequest>>` that:

- reads until `\r\n\r\n`;
- rejects headers over `MAX_LOCAL_HEADERS`;
- parses `Content-Length`;
- rejects config bodies over `MAX_LOCAL_CONFIG_BODY`;
- reads exactly the body length;
- returns method, path, and body strings.

- [ ] **Step 8: Implement route handling**

Add `handle_local_connection`, `local_http_response`, `local_json_response`, `local_asset_response`, and `content_type`.

Route behavior:

- `GET /sign-in` serves `PluginLocalAssets::get("index.html")`.
- `GET /assets/app.js` serves `PluginLocalAssets::get("assets/app.js")`.
- `GET /assets/styles.css` serves `PluginLocalAssets::get("assets/styles.css")`.
- any path containing `..` returns `400 {"error":"bad_request"}`.
- missing assets return `404 {"error":"not_found"}`.
- `GET /config` returns camelCase config plus `callbackUrl`.
- `POST /config` parses JSON, validates/normalizes both URLs, sets `user_selected = true`, and returns the same shape as `GET /config`.
- A successful `POST /config` also sets `using_default_web_server = false` and `using_default_hub_server = false` because the user has explicitly selected both targets.
- `GET /callback` returns small HTML and ignores query params.

- [ ] **Step 9: Add serial Rust lifecycle test**

In `crates/pandar-network-plugin/tests/http_boundary.rs`, add one test named `local_webserver_serves_sign_in_and_switches_target`.

The test should:

- call `pandar_plugin_start_local_webserver` with `web_configured=false`, `hub_configured=false`;
- parse `base_url`;
- assert it starts with `http://127.0.0.1:`;
- fetch `/sign-in`, `/assets/styles.css`, `/assets/app.js`, and `/config` using `std::net::TcpStream` helpers in the test;
- assert `/assets/styles.css` returns `content-type: text/css; charset=utf-8`;
- assert `/assets/app.js` returns `content-type: application/javascript; charset=utf-8`;
- assert `/missing.js` returns `404` and `/../Cargo.toml` returns a bad-request response without filesystem content;
- assert a request with headers larger than 16 KiB is rejected;
- assert `POST /config` with a body larger than 8 KiB is rejected;
- `POST /config` with switched URLs;
- assert `pandar_plugin_local_webserver_config` reports the switched `hub_url`;
- call start again with different URLs and assert user-selected config is not overwritten;
- assert `pandar_plugin_local_webserver_base_url` reports the same loopback base URL;
- assert `GET /callback?ticket=secret-ticket` returns the completion page and does not echo `secret-ticket`;
- assert invalid URLs fail;
- assert the initial config reports `using_default_web_server=true` and `using_default_hub_server=true`;
- assert a second start before user selection can update config/default flags by using a helper-only config builder test or by ordering this assertion before the `POST /config` call.

Use one serial test for global singleton mutation. Do not split config-mutating cases across parallel tests.

- [ ] **Step 10: Add non-global config/default test**

Add a pure helper test in `http_boundary.rs` or a crate unit test that does not touch `LOCAL_WEBSERVER`. It should build/normalize `LocalWebserverConfig` values for all four configured flag combinations:

| web_configured | hub_configured | using_default_web_server | using_default_hub_server |
| -------------- | -------------- | ------------------------ | ------------------------ |
| false          | false          | true                     | true                     |
| true           | false          | false                    | true                     |
| false          | true           | true                     | false                    |
| true           | true           | false                    | false                    |

If helper visibility would force over-exposure, keep the four flag assertions inside the single serial lifecycle test before `POST /config` and document that global mutation prevents parallel cases.

- [ ] **Step 11: Run focused plugin HTTP test**

Run:

```bash
cargo test -p pandar-network-plugin --test http_boundary local_webserver_serves_sign_in_and_switches_target -- --nocapture
```

Expected: the new lifecycle test passes.

## Task 3: C++ Shim Local Host Integration

**Files:**

- Modify: `crates/pandar-network-plugin/src/shim.cpp`
- Modify: `crates/pandar-network-plugin/tests/fixtures/studio_abi_probe.cpp`
- Modify: `crates/pandar-network-plugin/tests/studio_abi_probe.rs`

- [ ] **Step 1: Add Rust FFI declarations**

In `shim.cpp` extern block, add declarations for:

```cpp
PluginHttpResult pandar_plugin_start_local_webserver(
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    bool,
    bool
);
PluginHttpResult pandar_plugin_local_webserver_config();
```

- [ ] **Step 2: Extend Agent state**

Add fields:

```cpp
bool frontend_configured = false;
bool hub_configured = false;
```

- [ ] **Step 3: Add env resolution helper**

Add a helper returning value plus configured flag:

```cpp
std::pair<std::string, bool> env_or_default(const char* primary, const char* secondary, std::string fallback) {
    if (const char* value = std::getenv(primary); value && value[0] != '\0') return {value, true};
    if (const char* value = std::getenv(secondary); value && value[0] != '\0') return {value, true};
    return {std::move(fallback), false};
}
```

- [ ] **Step 4: Update agent creation**

In `bambu_network_create_agent`, set `frontend_url`, `hub_url`, `frontend_configured`, and `hub_configured` using plugin-specific env var precedence over generic app env vars.

- [ ] **Step 5: Start local webserver for Studio host**

Change `bambu_network_get_bambulab_host` to call `pandar_plugin_start_local_webserver` with both URLs and booleans, parse `base_url` from the JSON response using `field_from_json`, and return it. If the result is an error or `base_url` is empty, set `last_error` and return an empty string.

- [ ] **Step 6: Refresh selected hub URL before hub calls**

Add:

```cpp
void refresh_local_webserver_config(Agent* agent) {
    if (!agent) return;
    auto result = pandar_plugin_local_webserver_config();
    std::string body = body_from_result(result);
    if (result.status != 0) return;
    if (const auto hub = field_from_json(body, "hub_url"); !hub.empty()) agent->hub_url = hub;
}
```

Call it at public ABI entry points after `Agent* a = as_agent(agent)` and before invoking the existing `rust_*` helper that reads `hub_url`:

- `bambu_network_get_my_token`
- `bambu_network_get_user_print_info`
- `bambu_network_get_user_tasks`
- `bambu_network_start_print`
- `bambu_network_send_message_to_printer`

Do not mutate inside `rust_exchange_ticket`, `rust_get_printers`, `rust_get_jobs`, `rust_submit_print`, or `rust_submit_printer_operation`; those helpers can keep their existing `const Agent*` signatures.

Keep all new C++ helper code near the existing HTTP wrapper helpers around `body_from_result` and avoid unrelated ABI refactors. `shim.cpp` is already above the preferred size threshold, but this task must not split the Bambu Studio ABI shim because that would broaden the build-script and export risk beyond the requested change.

- [ ] **Step 7: Update ABI probe expectation**

In `studio_abi_probe.cpp`, change host validation to assert `out.host` starts with `http://127.0.0.1:` instead of matching `PANDAR_PLUGIN_FRONTEND_URL`.

- [ ] **Step 8: Set env in Rust ABI probe harness**

In `studio_abi_probe.rs`, when spawning the probe, set both:

```rust
.env("PANDAR_PLUGIN_FRONTEND_URL", frontend_url)
.env("PANDAR_PLUGIN_HUB_URL", mock_hub.url())
```

Keep the mock hub URL for HTTP exchange behavior.

- [ ] **Step 9: Run ABI probe**

Run:

```bash
cargo test -p pandar-network-plugin --test studio_abi_probe -- --nocapture
```

Expected: local ABI probe passes or skips only for missing C++ compiler/platform loader.

## Task 4: Documentation Updates

**Files:**

- Modify: `docs/architecture.md`
- Modify: `docs/development.md`
- Modify: `docs/compatibility/bambu-studio-plugin.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update architecture plugin paragraph**

In `docs/architecture.md`, update the `pandar-network-plugin` paragraph to mention Studio opens a loopback plugin webserver first, and that the plugin still talks only to `pandar-hub`.

- [ ] **Step 2: Update development plugin section**

In `docs/development.md` under `Bambu Studio Network Plugin`, replace the login flow steps with:

1. Studio opens `http://127.0.0.1:<port>/sign-in` from the plugin local server.
2. The local page shows/switches `PANDAR_PLUGIN_FRONTEND_URL`/`APP_BASE_URL` and `PANDAR_PLUGIN_HUB_URL`/`APP_API_URL` targets.
3. The local page continues to `<frontend>/plugin-sign-in?redirect_url=<local fallback callback>`.
4. Existing Studio `get_localhost_url` remains authoritative when available.
5. Studio calls token/profile ABI methods and the plugin exchanges with the selected hub API URL.

Also document no remote `/sign-in` fallback when loopback startup fails.

- [ ] **Step 3: Update compatibility docs**

In `docs/compatibility/bambu-studio-plugin.md`, add a short `Local Sign-In Host` section documenting:

- `PANDAR_PLUGIN_FRONTEND_URL` wins over `APP_BASE_URL`;
- `PANDAR_PLUGIN_HUB_URL` wins over `APP_API_URL`;
- Studio sign-in host is loopback `/sign-in`;
- no remote `/sign-in` fallback exists;
- real Studio sign-in evidence is still unverified until rows are updated.

- [ ] **Step 4: Update roadmap**

In `docs/roadmap.md`, add a completed bullet near the existing network plugin completed work:

```markdown
- Added a plugin-owned loopback sign-in webserver with an embedded `rust-embed` static target-selector page, preserving Studio's localhost ticket bridge while letting users switch Pandar frontend/API targets before login.
```

Do not mark real Studio sign-in compatibility as complete.

## Task 5: Full Verification And Cleanup

**Files:**

- All files changed above.

- [ ] **Step 1: Format**

Run:

```bash
cargo fmt
```

Expected: exits 0.

- [ ] **Step 1.5: Safety checklist before verification**

Read the diff and confirm:

- no direct MQTT, FTPS, SFTP, or printer sockets were added to `pandar-network-plugin`;
- no hub schema, migration, or plugin-ticket security rule changed;
- `bambu_network_get_bambulab_host` has no remote `/sign-in` fallback when loopback startup fails;
- `POST /config` marks config as user-selected and later start calls do not overwrite it;
- generated `frontend/plugin-local/dist` files match the source after `npm run build -w pandar-plugin-local --prefix frontend`;
- docs mention the local loopback host and do not claim real Studio sign-in compatibility is complete.

- [ ] **Step 2: Build plugin-local frontend**

Run:

```bash
npm run build -w pandar-plugin-local --prefix frontend
```

Expected: exits 0 and committed `dist` files are refreshed.

- [ ] **Step 3: Build existing frontend**

Run:

```bash
npm --prefix frontend run build
```

Expected: exits 0.

- [ ] **Step 4: Clippy**

Run:

```bash
cargo clippy
```

Expected: exits 0 with no warnings promoted to errors by project config.

- [ ] **Step 5: Workspace tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: exits 0.

- [ ] **Step 6: Review intended diff**

Run:

```bash
git status --short
git diff --check
```

Expected: only intended files are changed, including pre-existing user changes left untouched; `git diff --check` exits 0.
