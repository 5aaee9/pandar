# Bambu Studio Localized Plugin Login Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Pandar networking plugin login dialog render in Bambu Studio when Studio requests localized sign-in URLs such as `/en/sign-in`, and make Continue sign-in enter Bambu Studio's ticket-login callback flow.

**Architecture:** Keep the fix inside the plugin-local web boundary: root-relative embedded assets make the page location-independent, the Rust local webserver accepts only the explicitly supported localized sign-in route shape, and the embedded page asks Bambu Studio for its localhost callback URL before navigating to Pandar Web. Existing config, callback, and asset routes keep their current behavior.

**Tech Stack:** Rust `std::net` local HTTP server, Rust integration tests in `pandar-network-plugin`, static HTML under `frontend/plugin-local`.

---

### Task 1: Localized Sign-In Route And Asset Paths

**Files:**

- Modify: `crates/pandar-network-plugin/src/local_webserver.rs`
- Create: `crates/pandar-network-plugin/src/local_webserver/routes.rs`
- Modify: `crates/pandar-network-plugin/tests/local_webserver.rs`
- Modify: `frontend/plugin-local/src/index.html`
- Modify: `frontend/plugin-local/dist/index.html`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Write failing route and asset tests**

Add tests in `crates/pandar-network-plugin/tests/local_webserver.rs` near the existing local webserver routing test:

```rust
#[test]
fn localized_sign_in_route_serves_page_with_root_relative_assets() {
    let start = start_local(
        "http://localhost:3000/",
        "http://localhost:8080/",
        false,
        false,
    );
    let base_url = start["base_url"].as_str().unwrap();

    for path in ["/en/sign-in", "/en_GB/sign-in", "/zh-CN/sign-in"] {
        let response = get(&base_url, path);

        assert!(response.starts_with("HTTP/1.1 200 OK"), "{path}");
        assert!(response.contains("href=\"/assets/styles.css\""), "{path}");
        assert!(response.contains("src=\"/assets/app.js\""), "{path}");
    }
}

#[test]
fn malformed_localized_sign_in_routes_stay_not_found() {
    let start = start_local(
        "http://localhost:3000/",
        "http://localhost:8080/",
        false,
        false,
    );
    let base_url = start["base_url"].as_str().unwrap();

    for path in [
        "/1/sign-in",
        "/english/sign-in",
        "/en/sign-in/extra",
        "/en/assets/app.js",
        "/en/config",
        "/-/sign-in",
        "/中文/sign-in",
    ] {
        let response = get(&base_url, path);
        assert!(response.starts_with("HTTP/1.1 404 Not Found"), "{path}");
    }

    let traversal = get(&base_url, "/zh/../sign-in");
    assert!(traversal.starts_with("HTTP/1.1 400 Bad Request"));

    let post_sign_in = post_json(&base_url, "/en/sign-in", "{}");
    assert!(post_sign_in.starts_with("HTTP/1.1 404 Not Found"));
}
```

- [ ] **Step 2: Verify the tests fail for the expected reason**

Run:

```powershell
cargo test -p pandar-network-plugin localized_sign_in
```

Expected: `localized_sign_in_route_serves_page_with_root_relative_assets` fails because `/en/sign-in` currently returns `404` or the HTML still contains `./assets/...`.

- [ ] **Step 3: Make plugin-local assets root-relative**

Change `frontend/plugin-local/src/index.html` and `frontend/plugin-local/dist/index.html`:

```html
<link rel="stylesheet" href="/assets/styles.css" />
<script type="module" src="/assets/app.js"></script>
```

- [ ] **Step 4: Split local webserver route classification**

Create `crates/pandar-network-plugin/src/local_webserver/routes.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalRoute {
    SignIn,
    AppScript,
    Stylesheet,
    Config,
    Callback,
    BadRequest,
    NotFound,
}

pub fn classify(method: &str, path: &str) -> LocalRoute {
    if path.split('/').any(|segment| segment == "..") {
        return LocalRoute::BadRequest;
    }
    match (method, path) {
        ("GET", "/sign-in") => LocalRoute::SignIn,
        ("GET", path) if is_localized_sign_in_path(path) => LocalRoute::SignIn,
        ("GET", "/assets/app.js") => LocalRoute::AppScript,
        ("GET", "/assets/styles.css") => LocalRoute::Stylesheet,
        ("GET", "/config") => LocalRoute::Config,
        ("POST", "/config") => LocalRoute::Config,
        ("GET", "/callback") => LocalRoute::Callback,
        _ => LocalRoute::NotFound,
    }
}

fn is_localized_sign_in_path(path: &str) -> bool {
    let Some(locale) = path
        .strip_prefix('/')
        .and_then(|value| value.strip_suffix("/sign-in"))
    else {
        return false;
    };
    if locale.is_empty() || locale.contains('/') {
        return false;
    }
    let mut parts = locale.split(['_', '-']);
    let Some(language) = parts.next() else {
        return false;
    };
    let region = parts.next();
    if parts.next().is_some() {
        return false;
    }
    matches!(language.len(), 2 | 3)
        && language.bytes().all(|byte| byte.is_ascii_alphabetic())
        && region.is_none_or(|value| {
            value.len() == 2 && value.bytes().all(|byte| byte.is_ascii_alphabetic())
        })
}
```

Then update `crates/pandar-network-plugin/src/local_webserver.rs` to declare the module and use `routes::classify(...)` inside `route_local_request`:

```rust
mod routes;
```

```rust
    match routes::classify(request.method.as_str(), path) {
        routes::LocalRoute::BadRequest => local_json_response(400, stable_error_body("bad_request")),
        routes::LocalRoute::SignIn => local_asset_response("index.html"),
        routes::LocalRoute::AppScript => local_asset_response("assets/app.js"),
        routes::LocalRoute::Stylesheet => local_asset_response("assets/styles.css"),
        routes::LocalRoute::Config if request.method == "GET" => {
            let config = config.lock().expect("local webserver config").clone();
            local_json_response(200, http_config_body(base_url, &config))
        }
        routes::LocalRoute::Config => update_config(request, config, base_url),
        routes::LocalRoute::Callback => local_html_response(
            200,
            "<!doctype html><html><body><main>Sign-in request received. Return to Studio.</main></body></html>",
        ),
        routes::LocalRoute::NotFound => local_json_response(404, stable_error_body("not_found")),
    }
```

This focused split keeps `local_webserver.rs` below the 400 LOC threshold and does not use `include!`.

- [ ] **Step 5: Verify focused tests pass**

Run:

```powershell
cargo test -p pandar-network-plugin localized_sign_in
```

Expected: both localized sign-in tests pass.

- [ ] **Step 6: Update roadmap**

Add one completed bullet to `docs/roadmap.md`:

```markdown
- Fixed Bambu Studio localized plugin login URLs by serving `/<locale>/sign-in` from the local plugin webserver and making embedded plugin assets root-relative.
```

- [ ] **Step 7: Run full verification**

Run:

```powershell
cargo fmt
cargo clippy --workspace
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all commands pass.

- [ ] **Step 8: Bridge Continue sign-in to Bambu Studio's callback**

Modify `frontend/plugin-local/src/app.ts` and rebuild `frontend/plugin-local/dist/assets/app.js` so the Continue sign-in click:

- keeps the existing dirty-form guard;
- sends `{"command":"get_localhost_url","sequence_id":...}` through `window.wx.postMessage` when the Studio bridge exists;
- waits for the matching `get_localhost_url` response and uses `response.base_url + "/callback"` as the `redirect_url` for `/plugin-sign-in`;
- falls back to the local plugin callback URL outside Studio or when the bridge does not answer.

Run the plugin-local TypeScript build after the change. If the package-local `pnpm --dir frontend/plugin-local build` cannot find the workspace TypeScript binary, run the same build through `frontend/node_modules/.bin/tsc` and copy the static assets into `dist`.

- [ ] **Step 9: Manual plugin verification**

Rebuild and install the plugin, then launch Bambu Studio:

```powershell
$pluginPath = "$env:APPDATA\BambuStudio\plugins\bambu_networking.dll"
$configPath = "$env:APPDATA\BambuStudio\BambuStudio.conf"
Copy-Item -LiteralPath $pluginPath -Destination "$pluginPath.pre-pandar-check.bak" -Force
Copy-Item -LiteralPath $configPath -Destination "$configPath.pre-pandar-check.bak" -Force
cargo build -p pandar-app --target-dir target\pandar-install-check
cargo build -p pandar-network-plugin
target\pandar-install-check\debug\pandar.exe install-network-plugin --plugin-file target\debug\pandar_network_plugin.dll
```

Verify:

- installed `bambu_networking.dll` size and mtime match `target\debug\pandar_network_plugin.dll`
- after Bambu Studio starts, `bambu_networking.dll` is not overwritten
- installer-created `BambuStudio.conf.pandar-bak` exists, and the pre-check backup files exist before manual verification starts
- using Computer Use, clicking `Login/Register` renders the Pandar plugin login page instead of a blank dialog
- using Computer Use, clicking `Continue sign-in` enters Pandar Web with Studio's localhost callback URL so the ticket-login result can return to Bambu Studio

After the manual check, restore the previous Studio state unless the operator explicitly wants the Pandar plugin left installed:

```powershell
Copy-Item -LiteralPath "$pluginPath.pre-pandar-check.bak" -Destination $pluginPath -Force
Copy-Item -LiteralPath "$configPath.pre-pandar-check.bak" -Destination $configPath -Force
```
