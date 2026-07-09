# Pandar Android app

A Jetpack Compose + Material 3 Android client for `pandar-hub`. Located at
`mobile/android/` with package `zip.iptables.pandar.android`.

See also the design spec:
[`docs/superpowers/specs/2026-07-05-android-compose-app-design.md`](superpowers/specs/2026-07-05-android-compose-app-design.md).
The Hub browser login update is described in
[`docs/superpowers/specs/2026-07-08-android-hub-browser-login-design.md`](superpowers/specs/2026-07-08-android-hub-browser-login-design.md).

## Features (v1)

- Printers dashboard with live WebSocket updates.
- Per-printer detail: pause / resume / stop, chamber light, set hotend / bed /
  chamber temperature, AMS load / unload / reread RFID.
- Jobs list with retry-dispatch and reprint.
- Hub browser sign-in: the app opens `/mobile-sign-in`, receives a one-use
  Android callback ticket, exchanges it with the hub, and sends the returned
  tenant token as `Authorization: Bearer`.

## Build prerequisites

- Android Studio Ladybug or newer (or a JDK 17 + Android SDK 35 command-line setup).
- Android SDK platform 35 and build-tools.
- The Gradle wrapper scripts are committed, but the `gradle-wrapper.jar` binary
  is not. If it is missing, generate it once with:

  ```bash
  cd mobile/android
  gradle wrapper --gradle-version 8.10.2
  ```

  (or simply open the project in Android Studio, which supplies the wrapper jar).

## Build and test

```bash
cd mobile/android
./gradlew :app:testDebugUnitTest      # JVM unit tests
./gradlew :app:assembleDebug          # debug APK
./gradlew :app:lintDebug              # Android lint
```

The Rust workspace and the Next.js frontend are unaffected by this module; the
repo-wide `cargo fmt` / `cargo clippy --workspace` / `cargo nextest run
--workspace` and `npm run build:web` checks do not cover it.

## Configure the app

Open the app, go to **Settings**, and enter:

- **Hub base URL** — the public `pandar-hub` base URL (for example
  `https://hub.example.com/`). Used for both REST and the
  `/api/v1/tenants/{tenant_id}/printer-events` WebSocket.

Tap **Save**, then **Sign in**. The browser opens the Hub's `/mobile-sign-in`
page. After authentication and tenant selection, Hub redirects back to
`zip.iptables.pandar.android:/auth/callback` with a one-use ticket that the app
exchanges for its tenant token and tenant id.

## Architecture

Single-module app (`:app`), no Hilt. A small `AppContainer` created in
`PandarApplication` owns `SettingsRepository`, `AuthRepository`,
`PrinterEventsRepository`, and `PandarRepository`, plus the Retrofit `PandarApi`
which is rebuilt when the hub base URL changes. Networking uses OkHttp + Retrofit
+ kotlinx.serialization (single `appJson` instance for both encode and decode);
the WebSocket reuses the shared `OkHttpClient`. Status colors are always paired
with an icon and text label — never color alone.

Out of scope for v1: camera view, print dispatch / file upload, axis jog and
print-speed control, admin/users/agents management, internationalization, and
Keystore-backed encrypted token storage (tokens are stored in plain DataStore;
tracked as a follow-up).
