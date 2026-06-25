# Network Plugin Local Webserver Design

## Goal

Make `pandar-network-plugin` serve a local loopback sign-in page so Bambu Studio opens a plugin-owned URL, while that page lets the user see missing/default configuration, switch the Pandar target server, and continue into the existing tenant login-ticket flow.

## Scope

- Add a minimal loopback-only webserver to `crates/pandar-network-plugin`.
- Change the plugin host URL returned to Studio so `/sign-in` is served from that local webserver.
- Add a static frontend app under `frontend/` as a monorepo package for the plugin-local sign-in surface.
- Keep the existing Next.js `frontend/app/plugin-sign-in` tenant/ticket flow as the hub-authenticated continuation page.
- Do not add direct MQTT/SFTP/printer sockets to the plugin.
- Do not change hub database schema or plugin ticket security rules.

## User Flow

1. Studio asks the network plugin for the BambuLab host.
2. The plugin starts or reuses a loopback HTTP server and returns `http://127.0.0.1:<port>`.
3. Studio opens `/sign-in` from that local server.
4. The local page shows the current target Pandar web server and hub API server. If no explicit plugin server configuration exists, it shows that built-in defaults are being used and asks the user to confirm or replace them.
5. The user can switch the target server by editing the URL and selecting **Switch Target server**.
6. The local server stores the selected target URL in process memory for this plugin agent.
7. The user clicks **Continue sign-in**, which navigates to `<target-web-server>/plugin-sign-in?redirect_url=<local-callback>`.
8. The existing Next.js sign-in page keeps its current Studio `get_localhost_url` behavior. When Studio provides its own localhost callback, that Studio callback remains authoritative and may replace the local callback URL. When Studio does not provide one, the local callback URL is used as a browser fallback.
9. Studio calls `bambu_network_get_my_token(ticket)` with the ticket from its callback flow. The plugin exchanges that ticket against the currently selected hub API URL.

## Target Server Semantics

The plugin tracks two related targets:

- `web_url`: the Pandar frontend base URL that hosts `/plugin-sign-in`.
- `hub_url`: the Pandar Rust API base URL used for plugin ticket exchange and authenticated plugin API calls.

Defaults and env vars:

- `PANDAR_PLUGIN_FRONTEND_URL` or `APP_BASE_URL` sets `web_url`; otherwise default `http://localhost:3000`.
- `PANDAR_PLUGIN_HUB_URL` or `APP_API_URL` sets `hub_url`; otherwise default `http://localhost:8080`.
- Plugin-specific env vars win over generic app env vars: `PANDAR_PLUGIN_FRONTEND_URL` before `APP_BASE_URL`, and `PANDAR_PLUGIN_HUB_URL` before `APP_API_URL`.
- `web_configured` is `true` when `PANDAR_PLUGIN_FRONTEND_URL` or `APP_BASE_URL` supplied `web_url`; otherwise it is `false`.
- `hub_configured` is `true` when `PANDAR_PLUGIN_HUB_URL` or `APP_API_URL` supplied `hub_url`; otherwise it is `false`.
- `using_default_server` is `true` when either `web_configured` or `hub_configured` is `false`, because at least one built-in default is active.
- The JSON response also exposes `usingDefaultWebServer` and `usingDefaultHubServer` so the UI can name the missing/default side precisely.

Env precedence ownership lives in the C++ shim. Rust receives already-selected `web_url`, `hub_url`, `web_configured`, and `hub_configured` values through `pandar_plugin_start_local_webserver`, normalizes them, stores them, and reports default flags from those booleans.

The local sign-in page edits both values explicitly. To keep the first phase predictable, switching target server means submitting both `webUrl` and `hubUrl`. The UI may prefill `hubUrl` from the current/default value and labels it as the API server. The plugin updates the in-memory agent/server state so subsequent `bambu_network_get_my_token`, printer lookup, job lookup, print submission, and printer operation requests use the selected `hub_url`.

Config precedence and lifecycle:

- The first successful `pandar_plugin_start_local_webserver` seeds `web_url`, `hub_url`, `web_configured`, and `hub_configured` from the shim/env/default values.
- `POST /config` updates both URLs and marks the config as `user_selected`.
- Once `user_selected` is true, later `pandar_plugin_start_local_webserver` calls must not overwrite `web_url` or `hub_url` with shim/env/default values. They only return the existing server state.
- If `user_selected` is false, later `pandar_plugin_start_local_webserver` calls may update the config from supplied shim values. This supports tests and env changes before the user has chosen a target.
- The shim only reads `hub_url` back from the Rust config before hub-facing operations. It does not push agent state back into Rust except through `pandar_plugin_start_local_webserver`.

URL normalization:

- Trim surrounding whitespace and trailing `/` characters.
- Built-in defaults are normalized before storage, so the stored default frontend URL is `http://localhost:3000` even if existing shim code currently appends a trailing slash.
- Accept only `http` or `https` URLs with a host.
- Reject URLs with empty hosts or unsupported schemes.
- Return normalized URLs without trailing slashes from `/config`, then the UI builds `${webUrl}/plugin-sign-in`.

Security note: the loopback server has no authentication, so another local process owned by the user could change the target server and cause a later Studio ticket exchange to be sent to that selected hub URL. This is acceptable for this phase because the surface is loopback-only and same-user local processes are outside the plugin's current trust boundary, but the risk is explicit and should not be treated as remote-safe hardening.

## Local Webserver Ownership

The Rust plugin owns one process-wide local webserver singleton:

- Use `std::sync::OnceLock::get_or_init` to initialize the singleton on the first `pandar_plugin_start_local_webserver` call and avoid double-binding under concurrent calls.
- Store mutable config in `Arc<Mutex<LocalWebserverConfig>>`.
- Spawn one background OS thread that owns the loopback `TcpListener` and handles accepted connections with small blocking request parsing.
- Bound request parsing to small local requests: reject headers over 16 KiB and `POST /config` bodies over 8 KiB.
- The listener binds `127.0.0.1:0` once and lives until the plugin process exits. There is no stop/shutdown export in this phase.
- Later calls to `pandar_plugin_start_local_webserver` reuse the existing base URL and update the shared config with the supplied web/API URLs.
- Later start calls follow the config precedence rules above and must not overwrite `user_selected` values.
- All config reads and writes copy strings while holding the mutex briefly; request handling must not hold the lock while writing the HTTP response body.

This global process model matches the existing plugin ABI usage, where Studio creates one network-agent instance for the loaded plugin. It avoids passing Rust-owned handles through the C++ ABI.

## Local Webserver Contract

The Rust plugin exports small testable functions used by the C++ shim:

- `pandar_plugin_start_local_webserver(web_url_ptr, web_url_len, hub_url_ptr, hub_url_len, web_configured, hub_configured) -> PluginHttpResult`
  - Binds to `127.0.0.1:0`.
  - Returns JSON: `{"base_url":"http://127.0.0.1:<port>","web_url":"<url>","hub_url":"<url>","using_default_server":true|false,"using_default_web_server":true|false,"using_default_hub_server":true|false}`.
  - Reuses the same server after the first successful start.
- `pandar_plugin_local_webserver_base_url() -> PluginHttpResult`
  - Returns `{"base_url":"..."}` when running, otherwise `{"error":"local_webserver_unavailable"}` with non-zero status.
- `pandar_plugin_local_webserver_config() -> PluginHttpResult`
  - Returns `{"web_url":"<url>","hub_url":"<url>","using_default_server":true|false,"using_default_web_server":true|false,"using_default_hub_server":true|false}` when running, otherwise `{"error":"local_webserver_unavailable"}` with non-zero status.

The server exposes:

- `GET /sign-in`
  - Serves the static plugin-local HTML app.
- `GET /assets/app.js` and `GET /assets/styles.css`
  - Serve embedded files from `frontend/plugin-local/dist` through `rust-embed`.
  - `GET /sign-in` maps to embedded `index.html`.
  - Asset paths map to embedded `assets/app.js` and `assets/styles.css`.
  - Static paths are resolved relative to `dist` after trimming the leading `/`; path traversal segments (`..`) are rejected.
  - Use simple MIME types: `.html` -> `text/html; charset=utf-8`, `.css` -> `text/css; charset=utf-8`, `.js` -> `application/javascript; charset=utf-8`, `.json` -> `application/json; charset=utf-8`, and otherwise `application/octet-stream`.
  - Missing embedded assets return `404 {"error":"not_found"}`.
- `GET /config`
  - Returns `{"webUrl":"<url>","hubUrl":"<url>","usingDefaultServer":true|false,"usingDefaultWebServer":true|false,"usingDefaultHubServer":true|false,"callbackUrl":"<base>/callback"}`.
- `POST /config`
  - Accepts JSON `{"webUrl":"http://...","hubUrl":"http://..."}`.
  - Stores normalized URLs if both are `http` or `https` URLs with hosts.
  - Returns the same JSON shape as `GET /config` after a successful update.
  - Returns `400 {"error":"invalid_target_server"}` for invalid input.
- `GET /callback`
  - Returns a minimal completion page and ignores query parameters, including `ticket`. It does not store or exchange the ticket because Studio's ABI flow provides the ticket to `bambu_network_get_my_token` when the Studio bridge is available.

The server binds only to loopback. It does not persist the selected target server to disk in this phase.

JSON naming is intentionally split: exported Rust ABI helper responses use snake_case fields for C++ parsing, while browser HTTP responses use camelCase fields for the TypeScript page.

## Shim Behavior

- `bambu_network_create_agent` reads both frontend and hub env vars using the defaults above.
- The shim records and forwards `web_configured` and `hub_configured` independently, based on the env var rules above.
- `bambu_network_get_bambulab_host` starts the local webserver and returns its `base_url`.
- Before every hub-facing ABI operation, the shim calls `pandar_plugin_local_webserver_config`, parses `hub_url`, and copies it into the `Agent`. This makes switching target server affect subsequent ticket exchange and plugin API calls without restarting Studio.
- If `pandar_plugin_local_webserver_config` returns `local_webserver_unavailable`, the shim keeps the existing `Agent` URLs and continues the ABI operation with those values. This covers direct ABI tests and callers that invoke token/profile APIs before opening `/sign-in`; it does not reintroduce a remote `/sign-in` fallback.
- If the local server cannot start, the shim returns an empty host string and stores the local-server error in `last_error`. There is no remote frontend fallback for `/sign-in`; Studio-facing host resolution succeeds only when the loopback server starts.

## Existing Next.js Sign-in Behavior

Keep `frontend/app/plugin-sign-in/plugin-ticket-form.tsx` compatible with Studio's existing callback bridge:

- Preserve the `wx.postMessage({command:"get_localhost_url"})` request.
- Preserve replacing the form `redirect_url` with Studio's returned `base_url` when Studio responds.
- Treat the local page's `redirect_url=<local-callback>` as a fallback for non-Studio browser testing and Studio builds that do not return a callback URL.

Do not make the local callback override Studio's callback. The local webserver's required behavior is target selection and fallback completion, not replacing Studio's ticket handoff.

## Frontend Monorepo Page

Add `frontend/plugin-local/` as a small standalone package in the frontend monorepo:

- `frontend/package.json` adds `workspaces: ["plugin-local"]` while keeping the current Next.js app at the frontend root.
- `frontend/plugin-local/package.json` defines the static page package with `"name": "pandar-plugin-local"` and a `build` script.
- The plugin-local page uses plain TypeScript, HTML, and CSS with no new runtime dependencies.
- The build uses the TypeScript compiler already available in the frontend root dev dependencies: `tsc -p tsconfig.json` with `outDir` set to `dist/assets`, then copies `src/index.html` to `dist/index.html` and `src/styles.css` to `dist/assets/styles.css`.
- The build emits static files to `frontend/plugin-local/dist`, and the generated `dist` files are committed because `rust-embed` needs the folder to exist during `cargo build` on a fresh checkout.

The local page must:

- Show a clear default-configuration notice when `usingDefaultServer` is true.
- Name whether the web server, hub API server, or both are using defaults based on `usingDefaultWebServer` and `usingDefaultHubServer`.
- Show the current Pandar web server and hub API server.
- Provide editable web and API server inputs.
- Provide a **Switch Target server** button that calls `POST /config`.
- Provide a **Continue sign-in** link/button to `${webUrl}/plugin-sign-in?redirect_url=${callbackUrl}`.
- Use restrained product UI styling consistent with Pandar: dense, technical, accessible, and not a marketing page.

## Static Asset Embedding

The Rust plugin embeds the built plugin-local static files with the `rust-embed` crate. Use `#[derive(RustEmbed)]` and `#[folder = "../../frontend/plugin-local/dist/"]` in the plugin crate, then serve assets through the generated `Asset::get(path)` API. Cargo tests therefore require the static files to be present in the repository. The plugin-local package keeps generated output small and checked in for the plugin release artifact.

Dependency changes:

- Add `rust-embed` to workspace dependencies in the root `Cargo.toml`.
- Add `rust-embed.workspace = true` to `crates/pandar-network-plugin/Cargo.toml`.

Source of truth:

- Source files live under `frontend/plugin-local/src/`.
- Built files live under `frontend/plugin-local/dist/` and are committed. Expected output paths are `dist/index.html`, `dist/assets/styles.css`, and `dist/assets/app.js`.
- The current repo ignore rules do not ignore `frontend/plugin-local/dist`; if that changes later, add an explicit unignore rule before changing the embedded asset path.
- Developers update `dist` by running `npm run build -w pandar-plugin-local` from `frontend/` before Rust verification when source files change.
- A CI freshness check that compares `src` to `dist` is intentionally deferred for this phase; verification relies on running the plugin-local build before Rust checks when the local page changes.

## Tests

Rust tests cover:

- One serial Rust integration test covers the global singleton lifecycle to avoid `nextest` order dependence:
  - starting the local server returns a loopback base URL;
  - `GET /sign-in` returns HTML;
  - embedded CSS/JS assets referenced by the page are served with appropriate MIME types;
  - missing or traversal static asset paths return `404`/bad request without reading the filesystem;
  - `GET /config` shows the initial web/API targets and default/configured state;
  - `POST /config` switches the web/API target servers and marks config as user-selected;
  - a second `pandar_plugin_start_local_webserver` call reuses the first loopback base URL and does not overwrite user-selected config;
  - oversized local HTTP headers or config bodies are rejected without unbounded reads;
  - exported `pandar_plugin_local_webserver_config` reflects the switched hub API URL;
  - invalid target server input returns `400`;
  - `GET /callback?ticket=...` returns a completion page without storing or exchanging the ticket;
  - `pandar_plugin_local_webserver_base_url` reports the loopback base URL after startup;
  - `rust-embed` serves the committed static assets through the plugin binary.
- The serial Rust lifecycle test covers supplied `web_configured`/`hub_configured` flag combinations through the exported start/config APIs.
- The ABI probe covers shim env precedence and observes a loopback host rather than the remote frontend URL.

Frontend checks cover:

- `npm run build -w pandar-plugin-local` from `frontend/` builds the static page.
- Existing `npm --prefix frontend run build` still builds the Next.js app.

## Documentation

Update documentation for:

- `docs/roadmap.md`: record the local webserver sign-in flow and the remaining gap that real Bambu Studio compatibility still requires real Studio evidence.
- `docs/architecture.md` or the existing plugin development docs: describe that Studio host resolution now points to the plugin loopback webserver.
- `docs/compatibility/bambu-studio-plugin.md`: document `PANDAR_PLUGIN_FRONTEND_URL`, `PANDAR_PLUGIN_HUB_URL`, local loopback `/sign-in`, and the no-remote-fallback behavior when the local server cannot start.

Fresh verification must include `cargo fmt`, `cargo clippy`, and `cargo nextest run --manifest-path "Cargo.toml" --workspace` as required by the repository.

## Acceptance Criteria

- Studio-facing host resolution is local loopback.
- A user opening `/sign-in` locally can see whether default web/API target servers are in use.
- A user can switch both target server URLs from the local sign-in page.
- A user can continue to the selected frontend server's existing `/plugin-sign-in` flow.
- Subsequent plugin ABI calls use the selected hub API server.
- Existing plugin HTTP boundaries continue to pass.
- No new persistent database behavior is introduced.
