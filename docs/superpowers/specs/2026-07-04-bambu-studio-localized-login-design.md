# Bambu Studio Localized Plugin Login Design

## Goal

Fix the Bambu Studio login dialog for the Pandar networking plugin when Studio requests a localized sign-in route such as `/en/sign-in`, and make the local plugin page continue into the Studio-compatible ticket login flow.

## Context

The Pandar networking plugin starts a local HTTP server and returns that server as `bambu_network_get_bambulab_host`. Bambu Studio builds the login URL from that host. In the current English configuration, Studio requests a localized path (`/<language>/sign-in`) instead of only `/sign-in`. The plugin local server only serves `/sign-in`, so the Studio login WebView stays blank.

## Design

The plugin local webserver should serve the existing embedded sign-in page for both:

- `/sign-in`
- `/<locale>/sign-in`, where `<locale>` is one ASCII locale segment matching `[A-Za-z]{2,3}([_-][A-Za-z]{2})?`, such as `en`, `zh`, `en_GB`, or `zh-CN`

Only that exact localized sign-in shape becomes valid. Paths with extra segments, empty locale segments, non-ASCII locale text, numeric locale text, or other localized asset/config paths stay rejected as before.

No additional Hub auth behavior changes are needed.

The embedded login page must use root-relative asset URLs (`/assets/styles.css` and `/assets/app.js`) so the same HTML works when served from `/sign-in` or from `/<locale>/sign-in`. Do not add localized asset routes such as `/<locale>/assets/app.js`.

When the embedded login page runs inside Bambu Studio, the Continue sign-in action should use Studio's `window.wx.postMessage` bridge to request `get_localhost_url` before navigating to Pandar Web. The Pandar Web `/plugin-sign-in` URL should receive `redirect_url=<studio-localhost>/callback` when the bridge responds, so Bambu Studio's own HTTP login callback can exchange the plugin ticket through `bambu_network_get_my_token`. Outside Studio, or if the bridge does not respond, Continue sign-in should keep using the existing local plugin callback URL.

## Acceptance Criteria

- Installing `pandar_network_plugin.dll` as `bambu_networking.dll` still succeeds through `pandar install-network-plugin`.
- Starting Bambu Studio does not overwrite the installed Pandar plugin DLL.
- Clicking `Login/Register` in Bambu Studio renders the Pandar plugin login page instead of a blank dialog.
- Clicking `Continue sign-in` in the Bambu Studio login dialog navigates to Pandar Web with Studio's localhost callback URL when the Studio bridge is available.
- Existing `/sign-in`, `/assets/app.js`, `/assets/styles.css`, `/config`, `/callback`, and bad-path behavior remain unchanged except for the explicitly accepted `/<locale>/sign-in` route.
- The localized sign-in route is GET-only, matching the existing `/sign-in` route.
- Automated coverage proves the local webserver returns the embedded sign-in HTML for a localized sign-in route.
- Automated coverage proves the embedded sign-in HTML uses root-relative asset paths so localized sign-in pages load the same script and stylesheet as `/sign-in`.
- Automated coverage proves malformed localized sign-in paths still return `404`.

## Files

- Modify `crates/pandar-network-plugin/src/local_webserver.rs`.
- Modify `crates/pandar-network-plugin/tests/local_webserver.rs`.
- Modify `frontend/plugin-local/src/index.html`.
- Modify `frontend/plugin-local/src/app.ts`.
- Modify `frontend/plugin-local/dist/index.html`.
- Modify `frontend/plugin-local/dist/assets/app.js`.
- Update `docs/roadmap.md` after the fix is implemented.

## Verification

- Run a focused failing/passing test for localized sign-in route support.
- Run `cargo fmt`.
- Run `cargo clippy --workspace`.
- Run `cargo nextest run --manifest-path "Cargo.toml" --workspace`.
- Reinstall the plugin DLL, restart Bambu Studio, and verify the login page renders through Computer Use.
