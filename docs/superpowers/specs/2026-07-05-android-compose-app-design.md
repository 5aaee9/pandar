# Pandar Android App — Design / Spec

Status: Draft for independent SDD review.
Target location: `mobile/android/` (new Gradle module, package `zip.iptables.pandar.android`).

## 1. Goal

Build a native Android app that talks to an existing `pandar-hub` instance and lets an operator monitor and control Bambu printers: fleet/dashboard view, per-printer detail with temperatures and AMS controls, jobs list, and live updates over WebSocket. Authentication uses an external OIDC provider (Clerk/Logto/other) via Authorization Code + PKCE; the obtained access-token JWT is sent as `Authorization: Bearer` and verified by the hub against its JWKS (issuer/audience/scope), exactly like the web frontend.

This is a new, self-contained Gradle module. It does not change any Rust crate or the Next.js frontend.

## 2. Non-goals (v1, explicitly out of scope)

- Camera view (`camera.mp4`).
- Print dispatch / artifact upload.
- Axis jog (home/move X/Y/Z/E) and print-speed control.
- Admin/tenant/users/agents management screens.
- Internationalization / translations.
- Encrypted (Keystore-backed) token storage. (Plain DataStore in v1; flagged as follow-up.)
- Instrumented Android tests executed in CI (no Android SDK in this environment).
- Bambu machine MQTT/FTPS access; the app only talks to `pandar-hub`.

## 3. Inputs (existing, immutable contracts the app depends on)

All verified against the current codebase:

- `GET /api/v1/tenants/{tenant_id}/printers` → `{ printers: PrinterEventPrinter[] }`.
- `GET /api/v1/tenants/{tenant_id}/printers/{printer_id}` → `PrinterEventPrinter`.
- `GET /api/v1/tenants/{tenant_id}/agents` → `{ agents: Agent[] }` (`{id, tenant_id, name, status, created_at}`).
- `GET /api/v1/tenants/{tenant_id}/jobs` → `{ jobs: Job[] }` (full `Job`, `print`, `artifact`, `material` shape).
- Job-action routes (used by the Jobs screen, AC4). All return `CommandResponse` except where noted; all require Operator role; payloads are empty JSON (`{}`) or empty body:
  - `POST /api/v1/tenants/{tenant_id}/jobs/{job_id}/retry-dispatch` → `CommandResponse`.
  - `POST /api/v1/tenants/{tenant_id}/jobs/{job_id}/reprint` → `CommandResponse` (creates a new job; the returned command references the new dispatch).
  - `POST /api/v1/tenants/{tenant_id}/jobs/{job_id}/duplicate` → out of scope in v1 UI (documented for completeness).
- `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/controls` with JSON body `{action, ...}` → `CommandResponse`. Actions implemented in v1:
  - `pause`, `resume`, `stop`, `toggle_light`.
  - `set_chamber_light` (`{light_on: bool}`).
  - `set_hotend_temperature` (`{temperature_celsius, wait?, extruder_id?}`).
  - `set_bed_temperature` (`{temperature_celsius, wait?}`).
  - `set_chamber_temperature` (`{temperature_celsius, wait?}`).
  - `ams_reread_rfid` (`{ams_id, slot_id}`).
  - `ams_load_filament` (`{ams_id, slot_id, global_tray_id?, external_id?, extruder_id?}`).
  - `ams_unload_filament` (same fields as load).
- `GET /api/v1/tenants/{tenant_id}/printer-events` upgrades to a WebSocket. Each frame is one of:
  - `{type:"printer_snapshot", printer: PrinterEventPrinter}`
  - `{type:"job_progress", job: Job}`
  - `{type:"command_result", command: PrinterEventCommand}`
  The stream is best-effort and does not replay history; initial state is always loaded via REST.
- Auth: `Authorization: Bearer <jwt>` on every request (incl. WebSocket upgrade). The hub also supports `PANDAR_HUB_NO_AUTH=true` (no token needed); the app sends the token when configured and omits it when none is stored. On HTTP 401 the app attempts a single token refresh, then re-prompts sign-in.
- `PrinterEventPrinter` fields used by the UI: `id, tenant_id, agent_id, serial_number, name, model, status, last_seen_at, created_at, nozzle_temperatures[], active_nozzle, bed_temperature_celsius, bed_target_temperature_celsius, chamber_temperature_celsius, chamber_light_on, materials{ams_units, external_spools, active_tray, observed_at}`. (`nozzle_temperatures[].{label?, current_celsius?, target_celsius?}`.)
- Status → severity mapping (mirrors the `statusSeverity()` function in `frontend/app/dashboard-attention.ts` — note this is the pill-color function, distinct from `OFFLINE_PRINTER_STATUSES` which is only used for the offline-attention heuristic): failed/offline/unavailable/error/down → critical; warning/queued/sent/acknowledged/connecting/problem/degraded/pending → warning; online/ok/succeeded/completed/running/printing/ready → success; otherwise info. Unknown statuses never crash; they fall back to info and are still rendered as a pill with an icon + text label.

## 4. Architecture

Single-module Android app (`:app`). Layered:

```
ui (Compose + ViewModel)
  ↕ StateFlow / one-shot events
domain (pure Kotlin models + status mapping)
  ↕
data
  ├─ settings (DataStore: hub URL, tenant id, OIDC config, token refs)
  ├─ auth (AppAuth OIDC: authorize, exchange, refresh, sign out)
  └─ remote (Retrofit REST + OkHttp WebSocket)
  ↕
core (AppContainer manual DI, logger)
```

No Hilt — a small `AppContainer` created in `Application.onCreate()` and passed via `CompositionLocal`/ViewModel factory. Keeps the module lean and avoids annotation-processor setup.

Threading: coroutines + Flow. Networking on `Dispatchers.IO`; WebSocket frames collected on a long-lived `CoroutineScope` owned by a `PrinterEventsRepository`.

### 4.1 Authentication

- OIDC via AppAuth-Android (`net.openid:appauth`) and Chrome Custom Tabs. Redirect URI scheme `zip.iptables.pandar.android` (e.g. `zip.iptables.pandar.android:/oauth2redirect`) declared as an intent-filter `VIEW` with `android:scheme` and (for app-links-style robustness) `android:autoVerify="false"`.
- Configurable: discovery URL (AppAuth fetches issuer + JWKS + endpoints), clientId, scopes, redirect URI, plus hub base URL and tenant id. All stored in DataStore.
- After the authorization code is exchanged, the resulting **access token (JWT)** is stored and used as the Bearer credential. Refresh token used on 401 / proactively near expiry.
- "Sign out" revokes via AppAuth end-session when the discovery doc exposes one; tokens are discarded regardless of revocation success.
- A no-auth hub is supported by leaving OIDC unconfigured — the app then issues requests with no `Authorization` header.

### 4.2 Networking

- Retrofit + `Json` (kotlinx.serialization) for REST. A `RequestInterceptor` adds `Authorization: Bearer <token>` when a token is present.
- One `OkHttpClient` (shared, with timeouts and the interceptor) reused by Retrofit and the WebSocket.
- WebSocket: `PrinterEventsRepository.connect(tenantId)` opens `ws(s)://<hub>/api/v1/tenants/<tenant>/printer-events`, parses each text frame into a sealed `PrinterEvent`, and emits into a `SharedFlow`. Reconnect with capped exponential backoff (e.g. 1s..30s) until explicitly stopped.
- WebSocket auth-refresh interaction: the WS is opened with the current Bearer token in the upgrade request. If the upgrade is rejected with an HTTP 401/403 (or the socket closes immediately after upgrade with no frames), the repository triggers the same single-refresh path used for REST 401; if refresh succeeds it reconnects immediately (resetting backoff), otherwise it surfaces a re-sign-in signal and continues backoff until a new token is set.

### 4.3 Data model (DTOs and domain models)

DTOs under `data.remote.dto` mirror hub JSON 1:1 (`@Serializable`, nullable where the hub is nullable). A pure mappers layer converts DTO → domain model under `domain.model` (e.g. `Printer`, `Job`, `Agent`, `Command`). Keeping DTOs and domain models separate lets unit tests pin JSON shapes without entangling Compose.

### 4.4 AMS materials schema and control-payload mapping

The hub serializes `materials` as raw JSON (`serde_json::Value`) for `ams_units`, `external_spools`, and `active_tray`. The app deserializes them defensively (lenient: unknown/missing fields never throw). The shapes the app targets (verified from the Next.js `PrinterMaterials` type and `MaterialSnapshot`) are:

```
materials = {
  ams_units: [
    {
      unit_id?: string,
      humidity?: number|string|null,
      humidity_level?: number|string|null,
      temperature_celsius?: number|string|null,
      toolhead?: string|null,
      trays?: [
        {
          tray_id?: string,
          type?: string|null,          // filament type, e.g. "PLA"
          color?: string|null,          // hex color, e.g. "#FFFFFF"
          multi_color?: string[]|null,
          filament_id?: string|null,
          name?: string|null,
          remaining_estimate?: string|number|null,
          k_value?: string|number|null,
          toolhead?: string|null,
          global_tray_id?: number|null,
          exists?: boolean|null
        }
      ]
    }
  ],
  external_spools: [
    { external_id?: string, tray_id?: string, type?, color?, multi_color?, filament_id?, name?, remaining_estimate?, k_value?, toolhead?, global_tray_id?, exists? }
  ],
  active_tray: {
    kind?: string,                         // e.g. "ams" | "external"
    ams_id?: string|null,
    tray_id?: string|null,
    global_tray_id?: number|null,
    external_id?: string|null
  } | null,
  observed_at: string
}
```

Mapping rules used to build AMS control payloads (see §3 action field requirements):

- For an AMS tray inside `ams_units[i].trays[j]`:
  - `ams_id` ← `ams_units[i].unit_id` (string).
  - `slot_id` ← `trays[j].tray_id` (string).
  - `global_tray_id` ← `trays[j].global_tray_id` (number, optional).
  - `external_id` ← not set for AMS trays.
- For an external spool in `external_spools[k]`:
  - `external_id` ← `external_spools[k].external_id` (string).
  - **External spools are display-only in v1.** The hub's `ams_load_filament` / `ams_unload_filament` actions require `ams_id` AND `slot_id` to be present (verified in `crates/pandar-hub/src/routes/printer_operations.rs`), and external spools have neither. `external_id` is accepted by those actions only as an optional companion to a valid `ams_id`/`slot_id`. Therefore external spool rows render their color/type/name/remaining/k_value and the active indicator, but show a "Display only" caption instead of Load/Unload buttons.

Behavior when fields are absent or unknown:

- Unknown top-level or nested keys are ignored (lenient deserialization).
- If a tray lacks `unit_id`/`tray_id` (AMS) or `external_id` (external spool), that tray's action buttons (load/unload/reread) are **disabled** with an explanatory caption; the rest of the tray's display (color/type/remaining) still renders.
- `color` is rendered as a small color swatch AND a text hex label (never color alone, per DESIGN.md). `type` and `name` are shown as text. `remaining_estimate`, `k_value` are shown when present and otherwise omitted.
- `active_tray` highlights the currently active tray with an icon + "Active" label.
- If the whole `materials` object is `null`, the AMS section renders an empty state ("No material data") and all action buttons are hidden.

## 5. UI (Compose + Material 3, Navigation-Compose)

Bottom-nav destinations: **Printers**, **Jobs**, **Settings**. A sign-in gate routes to **Login** when OIDC is configured but no valid token exists.

1. **Login** — short explanation + "Sign in" button that triggers AppAuth; surfaces errors. Reachable when configured-but-unauthenticated.
2. **Settings** — fields for hub base URL, tenant id, OIDC discovery URL, client id, scopes, redirect URI; Sign in / Sign out; shows current identity (sub/issuer) if available. Edits persist to DataStore.
3. **Printers (dashboard)** — summary strip (total printers, online printers, agents connected) + lazy column of printer cards. Each card: status pill (icon + label), name, model, serial (monospace), key temps (bed current/target, active hotend current/target), chamber-light indicator. Pull-to-refresh re-fetches REST and forces a WS reconnect if down.
4. **Printer detail** — full nozzle temperature list, bed/chamber current vs target, **set hotend/bed/chamber temperature** controls (numeric field + Apply), **chamber light** toggle, **pause/resume/stop** actions, AMS section with trays (color/type/remaining) and **load / unload / reread RFID** per tray. Actions call `POST .../controls`; a snackbar/toast shows command id + status; command status updates arrive over the WS `command_result` event and update the surface.
5. **Jobs** — lazy column of jobs: filename, status pill, progress %, remaining time, current/total layer, created/updated timestamps. For each non-terminal job the screen offers **Retry dispatch** and **Reprint** actions wired to `POST .../retry-dispatch` and `POST .../reprint` (see §3 job-action routes). Both require Operator role, which the hub enforces; the app shows the returned `CommandResponse` status in a snackbar and optimistically refreshes the jobs list (REST + the WS `command_result`/`job_progress` events update it). Action buttons are disabled while a request is in flight and when the job is in a terminal state with nothing to retry.

Accessibility: status pills always carry icon + text (never color alone); minimum 48dp touch targets; content descriptions on icons; respects system dark/light.

### Theme

Material 3 with a neutral monochrome scheme derived from `DESIGN.md` (white surfaces, near-black ink, hairline borders; dark mode mirrors the dark palette). Status colors (success/warning/critical) used sparingly and paired with icon+label. Inter is requested via system sans fallback; a monospace `FontFamily` is used for serial numbers / ids.

## 6. Persistence

Jetpack DataStore (Preferences) under the app's files. Keys: `hubBaseUrl`, `tenantId`, `oidcDiscoveryUrl`, `oidcClientId`, `oidcScopes`, `oidcRedirectUri`, `accessToken`, `refreshToken`, `tokenExpiresAt`. No encryption in v1 (documented follow-up).

## 7. Build & project layout

```
mobile/android/
  settings.gradle.kts
  build.gradle.kts            (root, plugins only)
  gradle.properties
  gradle/libs.versions.toml   (version catalog)
  gradle/wrapper/gradle-wrapper.{jar,properties}
  gradlew, gradlew.bat
  app/
    build.gradle.kts
    proguard-rules.pro
    src/main/AndroidManifest.xml
    src/main/res/...           (themes, strings, icons)
    src/main/kotlin/zip/iptables/pandar/android/
      PandarApplication.kt
      MainActivity.kt
      core/{di/AppContainer.kt, util/Logger.kt}
      data/
        settings/SettingsRepository.kt
        auth/{AuthRepository.kt, AuthState.kt}
        remote/
          ApiModule.kt            (OkHttp + Retrofit construction)
          PandarApi.kt            (Retrofit interface)
          dto/*                   (DTOs + serializers)
          ws/PrinterEventsRepository.kt
      domain/
        model/*                   (Printer, Job, Agent, Command, Severity)
        status/StatusMeta.kt
      ui/
        theme/{Color.kt, Theme.kt, Type.kt}
        navigation/PandarNavGraph.kt
        components/{StatusPill.kt, MonoText.kt, ...}
        login/LoginScreen.kt + ViewModel
        settings/SettingsScreen.kt + ViewModel
        printers/{PrintersScreen.kt, PrintersViewModel.kt, PrinterCard.kt}
        printerdetail/{PrinterDetailScreen.kt, PrinterDetailViewModel.kt}
        jobs/{JobsScreen.kt, JobsViewModel.kt}
    src/test/kotlin/zip/iptables/pandar/android/
      domain/status/StatusMetaTest.kt
      data/remote/dto/*DtoTest.kt (sample JSON → DTO)
      data/remote/PrinterEventsDecoderTest.kt
      data/remote/ControlsBodyShapeTest.kt
```

Gradle: Kotlin DSL + version catalog. Kotlin 2.0.x, AGP 8.x, Compose Compiler plugin, `compileSdk`/`targetSdk` 35, `minSdk` 26. Java toolchain 17.

Because this environment has no Android SDK / JDK, the Gradle build is **not** run here. Verification in this environment is JVM unit tests only if a JDK is installable; otherwise the unit tests are authored and documented as the build-time verification to be run in Android Studio. The repo's existing `cargo`/`clippy`/`nextest` checks do not cover this module and are unaffected.

## 8. Verification strategy

Environment constraint: no `java`, no `ANDROID_HOME`. Two tiers:

1. **Authored unit tests (JVM, run in Android Studio / CI later):**
   - `StatusMetaTest`: every documented status token maps to the expected severity; unknown tokens map to info and never throw.
   - DTO tests: sample JSON payloads (printer list, job list, agent list, the three WS event variants) deserialize to the expected domain values; nullable fields absent/`null` resolve to `null`.
   - `PrinterEventsDecoderTest`: each `{type:...}` variant routes to the correct sealed branch.
   - `ControlsBodyShapeTest`: each implemented control action serializes to the exact JSON the hub's `PrinterOperationRequest` expects (e.g. `set_bed_temperature` ⇒ `{"action":"set_bed_temperature","temperature_celsius":60,"wait":false}` with no stray keys — the hub uses `#[serde(deny_unknown_fields)]`-equivalent strict parsing per action).
2. **Static correctness review** of Gradle files, manifest, and Compose code by independent reviewers.

`cargo`/`clippy`/`cargo nextest` remain green because no Rust code is touched. `npm run build:web` is unaffected.

## 9. Acceptance criteria

The spec is satisfied when:

- AC1. A new `mobile/android/` Gradle module exists with the package `zip.iptables.pandar.android`, builds conceptually (reviewed Gradle config), and is referenced from a top-level `README`/`docs/development.md` note pointing to Android Studio for the actual build.
- AC2. Printers screen loads `GET .../printers` and renders the fleet with status pills (icon + label), names, serials (monospace), and key temperatures.
- AC3. Printer detail supports pause/resume/stop, chamber-light on/off, set hotend/bed/chamber temperature, AMS reread/load/unload — each producing the exact JSON the hub accepts.
- AC4. Jobs screen lists jobs with status, progress, remaining time, and exposes retry/reprint.
- AC5. WebSocket `/printer-events` connects with the Bearer token, parses all three event variants, updates the relevant UI, and reconnects with capped backoff.
- AC6. OIDC sign-in (Authorization Code + PKCE) produces an access-token JWT used as Bearer; sign-out discards tokens; no-auth hubs work with OIDC unconfigured.
- AC7. Unit tests from §8 (1) are present and document the expected JVM verification commands.
- AC8. Theme follows the neutral Material 3 palette from `DESIGN.md`; status never relies on color alone.
- AC9. `cargo fmt`, `cargo clippy --workspace`, `cargo nextest run --workspace`, and `npm run build:web` are unaffected (Rust/TS untouched).

## 10. Risks & rollback

- **No Android toolchain in CI.** Mitigation: authored unit tests + independent review; document that the build/tests must run in Android Studio. Rollback = delete `mobile/android/`.
- **OIDC provider quirks.** Mitigation: discovery-driven config (endpoints/JWKS from discovery doc), redirect URI configurable; documented provider-specific notes.
- **Hub status string drift.** Mitigation: unknown tokens degrade to `info`, never crash.
- **Token storage is plaintext in v1.** Accepted risk for an operator-focused tool; flagged as follow-up to move to EncryptedSharedPreferences/Keystore.
- **Strict hub parsing (`deny_unknown_fields`-equivalent per action).** Mitigation: per-action request bodies are unit-tested for exact key sets.

## 11. Docs impact

- Add a short section in `docs/development.md` (or a new `docs/android.md`) describing the module, build prerequisites (Android Studio / JDK 17 / SDK 35), and how to configure hub + OIDC. Reference the design spec path.
- Update `docs/roadmap.md` with the completed Android app entry under the appropriate section.
- No Rust/TS doc changes.

## 12. Open questions resolved during brainstorming

- Auth: **OIDC** (Clerk/Logto/other) via AppAuth + PKCE.
- Operation scope v1: **temperatures + AMS load/unload/reread** (plus core pause/resume/stop/light).
- Live updates: **WebSocket** with REST fallback.
- Camera: **not in v1**.
