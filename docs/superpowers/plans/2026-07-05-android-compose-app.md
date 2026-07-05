# Pandar Android App Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a self-contained Jetpack Compose + Material 3 Android Gradle module under `mobile/android/` (package `zip.iptables.pandar.android`) that monitors and controls Bambu printers via the existing `pandar-hub` HTTP/WebSocket API, authenticated with an external OIDC provider (Authorization Code + PKCE) whose access-token JWT is sent as `Authorization: Bearer`.

**Architecture:** Layered single-module (`:app`) app: `core` (manual `AppContainer` DI), `data` (DataStore settings, AppAuth OIDC auth, Retrofit REST + OkHttp WebSocket), `domain` (pure Kotlin models + status mapping), `ui` (Compose + Material 3 + Navigation-Compose + ViewModels). No Hilt.

**Tech Stack:** Kotlin 2.0.21, AGP 8.7.x, Compose BOM 2024.10.x, Material 3, Navigation-Compose, Coroutines/Flow, Retrofit + kotlinx.serialization + OkHttp, AppAuth-Android, DataStore Preferences. compileSdk/targetSdk 35, minSdk 26, JDK 17.

## Global Constraints

- Package: `zip.iptables.pandar.android`. Location: `mobile/android/`.
- **AC9 (hard invariant):** Do NOT modify any Rust crate or any Next.js/frontend file. `cargo fmt`, `cargo clippy --workspace`, `cargo nextest run --workspace`, and `npm run build:web` must remain unaffected. The only non-`mobile/android/` edits allowed are docs: `docs/development.md` (or new `docs/android.md`), `docs/roadmap.md`, `.gitignore`, `README.md`.
- Hub HTTP/WS contracts are immutable; see spec §3 and the file map below. The hub's `PrinterOperationRequest` enforces strict per-action field validation (extra JSON keys are rejected). Control request bodies MUST contain exactly the keys the action requires.
- The hub verifies raw JWT access tokens via remote JWKS with issuer/audience/scope checks; the app sends the obtained access-token JWT as `Authorization: Bearer <jwt>`.
- Environment constraint: **no `java`, no `ANDROID_HOME`/Android SDK in the build env.** Gradle build/test execution is deferred to Android Studio. In-session verification = static review + authored unit tests. The plan's "Run test" steps document the exact Gradle command to run in Android Studio (`./gradlew :app:testDebugUnitTest` or `./gradlew :app:testDebugUnitTest --tests <FQN>`); in this env, the verifier confirms the test file exists and its assertions/expected values are correct by inspection.
- Status pills always carry an icon + text label, never color alone (DESIGN.md / WCAG 2.2 AA).
- Status → severity mapping (mirrors the `statusSeverity()` function in `frontend/app/dashboard-attention.ts` — the pill-color function, distinct from `OFFLINE_PRINTER_STATUSES` which is only the offline-attention heuristic): failed/offline/unavailable/error/down → CRITICAL; warning/queued/sent/acknowledged/connecting/problem/degraded/pending → WARNING; online/ok/succeeded/completed/running/printing/ready → SUCCESS; otherwise INFO. (Note: `problem` is WARNING, not CRITICAL; `failed` is CRITICAL.) Unknown tokens never throw (→ INFO).
- AMS materials (`ams_units`/`external_spools`/`active_tray`) are deserialized leniently; unknown/missing fields never throw. Mapping: ams_id←unit_id, slot_id←tray_id, global_tray_id←global_tray_id, external_id←external_id. Trays missing the id needed for an action disable that action's button with a caption. Color is always paired with a hex label.
- Git commits follow Conventional Commits (repo convention). Commit per task or logical group.

## Hub contract reference (from current codebase — implement against these)

REST (Bearer auth; `tenant_id` is a UUID string):

- `GET /api/v1/tenants/{tenant_id}/printers` → `{ "printers": [PrinterEventPrinter] }`
- `GET /api/v1/tenants/{tenant_id}/printers/{printer_id}` → `PrinterEventPrinter`
- `GET /api/v1/tenants/{tenant_id}/agents` → `{ "agents": [{id, tenant_id, name, status, created_at}] }`
- `GET /api/v1/tenants/{tenant_id}/jobs` → `{ "jobs": [Job] }`
- `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/controls` body `{action, ...}` → `CommandResponse`
- `POST /api/v1/tenants/{tenant_id}/jobs/{job_id}/retry-dispatch` (empty body) → `CommandResponse`
- `POST /api/v1/tenants/{tenant_id}/jobs/{job_id}/reprint` (empty body) → `CommandResponse`

Control actions + exact key sets (extra keys rejected):
- `pause` / `resume` / `stop` / `toggle_light` → `{"action":"<action>"}` (no other keys).
- `set_chamber_light` → `{"action":"set_chamber_light","light_on":<bool>}`.
- `set_print_speed` → (out of v1 scope).
- `set_hotend_temperature` → `{"action":"set_hotend_temperature","temperature_celsius":<u16>,"wait":<bool>,"extruder_id":<u32?>}` (`wait` and `extruder_id` optional; when `extruder_id` is null it must be omitted, not `null`, because of strict validation — verify against hub: the hub uses `Option<u32>` with `#[serde(default)]`, so `null` is accepted; but to be safe, omit when absent).
- `set_bed_temperature` → `{"action":"set_bed_temperature","temperature_celsius":<u16>,"wait":<bool>}`.
- `set_chamber_temperature` → `{"action":"set_chamber_temperature","temperature_celsius":<u16>,"wait":<bool>}`.
- `ams_reread_rfid` → `{"action":"ams_reread_rfid","ams_id":<u32>,"slot_id":<u32>}`.
- `ams_load_filament` → `{"action":"ams_load_filament","ams_id":<u32>,"slot_id":<u32>}` plus optional `global_tray_id` (u32), `external_id` (string), `extruder_id` (u32) ONLY when present (omitted when null).
- `ams_unload_filament` → same shape and optionality as load (`ams_id`+`slot_id` required; `global_tray_id`/`external_id`/`extruder_id` optional, omitted when null).

> Note on numeric ids: the hub types `ams_id: Option<u32>`, `slot_id: Option<u32>`, `global_tray_id: Option<u32>`, `temperature_celsius: Option<u16>`, `extruder_id: Option<u32>`, `speed_mode: Option<u8>`, `feedrate_mm_per_min: Option<u32>`. The materials JSON carries `unit_id`/`tray_id`/`global_tray_id` as **strings or numbers** (lenient). The app must coerce to the integer the hub requires; if a tray id cannot be parsed as a positive integer, the action button is disabled (per spec §4.4).

WS: `GET /api/v1/tenants/{tenant_id}/printer-events` upgrades with Bearer auth; frames are one of `{"type":"printer_snapshot","printer":{...}}`, `{"type":"job_progress","job":{...}}`, `{"type":"command_result","command":{...}}`.

`PrinterEventPrinter`: `{ id, tenant_id, agent_id, serial_number, name, model: string|null, status, last_seen_at, created_at, nozzle_temperatures: [{label?:string|null, current_celsius?:string|null, target_celsius?:string|null}], active_nozzle: string|null, bed_temperature_celsius: string|null, bed_target_temperature_celsius: string|null, chamber_temperature_celsius: string|null, chamber_light_on: bool|null, materials: PrinterMaterials|null }`.

`PrinterMaterials`: see spec §4.4.

`Job`: see `frontend/app/dashboard-types.ts` (`id, printer_id, agent_id, artifact_id, command_id, status, error, created_at, updated_at, print:{...}, command:{id,kind,status}, artifact:{id,tenant_id,filename,content_type,size_bytes,metadata:null,created_at}, material:{...}`).

---

## File Structure

```
mobile/android/
  .gitignore
  settings.gradle.kts
  build.gradle.kts
  gradle.properties
  gradlew, gradlew.bat
  gradle/libs.versions.toml
  gradle/wrapper/gradle-wrapper.properties
  gradle/wrapper/gradle-wrapper.jar   (binary — added from a standard wrapper)
  app/
    build.gradle.kts
    proguard-rules.pro
    src/main/AndroidManifest.xml
    src/main/res/values/{strings,themes}.xml
    src/main/res/values-night/themes.xml
    src/main/res/xml/backup_rules.xml
    src/main/res/mipmap-*/ic_launcher (use default adaptive icon via mipmap-anydpi-v26)
    src/main/kotlin/zip/iptables/pandar/android/
      PandarApplication.kt
      MainActivity.kt
      core/util/Logger.kt
      core/di/AppContainer.kt
      data/settings/SettingsRepository.kt
      data/settings/SettingsSnapshot.kt
      data/auth/AuthRepository.kt
      data/auth/AuthEvent.kt
      data/remote/Json.kt
      data/remote/ApiModule.kt
      data/remote/BearerAuthInterceptor.kt
      data/remote/PandarApi.kt
      data/remote/dto/Dtos.kt
      data/remote/dto/PrinterMaterialsDto.kt
      data/remote/ws/PrinterEventsRepository.kt
      data/repository/PandarRepository.kt
      domain/model/Models.kt
      domain/model/Severity.kt
      domain/status/StatusMeta.kt
      ui/theme/Color.kt
      ui/theme/Type.kt
      ui/theme/Theme.kt
      ui/navigation/PandarNavGraph.kt
      ui/components/StatusPill.kt
      ui/components/FormFields.kt
      ui/login/LoginScreen.kt
      ui/login/LoginViewModel.kt
      ui/settings/SettingsScreen.kt
      ui/settings/SettingsViewModel.kt
      ui/printers/PrintersScreen.kt
      ui/printers/PrintersViewModel.kt
      ui/printers/PrinterCard.kt
      ui/printerdetail/PrinterDetailScreen.kt
      ui/printerdetail/PrinterDetailViewModel.kt
      ui/jobs/JobsScreen.kt
      ui/jobs/JobsViewModel.kt
    src/test/kotlin/zip/iptables/pandar/android/
      domain/status/StatusMetaTest.kt
      data/remote/dto/PrinterListDtoTest.kt
      data/remote/dto/JobsListDtoTest.kt
      data/remote/dto/AgentsListDtoTest.kt
      data/remote/dto/PrinterEventsDecoderTest.kt
      data/remote/ControlsBodyShapeTest.kt
      data/settings/SettingsMappingTest.kt
docs/android.md          (new)
docs/roadmap.md          (modify — append Android entry)
README.md                (modify — add mobile/android pointer)
.gitignore               (modify — add mobile/android local Gradle ignores)
```

---

## Task 1: Gradle skeleton, version catalog, wrapper, manifest, theme resources

**Files:**
- Create: `mobile/android/.gitignore`
- Create: `mobile/android/settings.gradle.kts`
- Create: `mobile/android/build.gradle.kts`
- Create: `mobile/android/gradle.properties`
- Create: `mobile/android/gradle/libs.versions.toml`
- Create: `mobile/android/gradle/wrapper/gradle-wrapper.properties`
- Create: `mobile/android/gradlew`, `mobile/android/gradlew.bat`
- Create: `mobile/android/app/build.gradle.kts`
- Create: `mobile/android/app/proguard-rules.pro`
- Create: `mobile/android/app/src/main/AndroidManifest.xml`
- Create: `mobile/android/app/src/main/res/values/strings.xml`
- Create: `mobile/android/app/src/main/res/values/themes.xml`
- Create: `mobile/android/app/src/main/res/values-night/themes.xml`
- Create: `mobile/android/app/src/main/res/xml/backup_rules.xml`
- Create: `mobile/android/app/src/main/res/mipmap-anydpi-v26/ic_launcher.xml`, `ic_launcher_round.xml`
- Create: `mobile/android/app/src/main/res/drawable/ic_launcher_foreground.xml`
- Create: `mobile/android/app/src/main/res/values/ic_launcher_background.xml`
- Modify: `/.gitignore` (root) — append Android Gradle local ignores (delegated to `mobile/android/.gitignore` instead; root untouched per AC9 except docs).

**Interfaces:**
- Produces: a buildable Gradle project skeleton with applicationId `zip.iptables.pandar.android`, the Compose/Material3/Retrofit/AppAuth/DataStore dependencies wired via the version catalog, JDK 17 toolchain, namespace `zip.iptables.pandar.android`.

- [ ] **Step 1: Create `gradle/libs.versions.toml`** with versions + libraries: AGP 8.7.3, Kotlin 2.0.21, Compose Compiler plugin 2.0.21 (via `org.jetbrains.kotlin.plugin.compose`), Compose BOM 2024.10.01, Material3, Navigation-Compose 2.8.4, lifecycle-viewmodel-compose 2.8.7, activity-compose 1.9.3, coroutines 1.9.0, kotlinx-serialization 1.7.3 + plugin, retrofit 2.11.0 + retrofit2-kotlinx-serialization-converter 1.0.0, okhttp 4.12.0 (incl. logging-interceptor), datastore-preferences 1.1.1, appauth 0.11.1, browser 1.8.0. Declare bundles for `compose`, and `test` (junit 4.13.2, kotlinx-coroutines-test 1.9.0, okhttp mockwebserver 4.12.0, robolectric 4.13 — optional).

- [ ] **Step 2: Create root `settings.gradle.kts`** with `pluginManagement`/`dependencyResolutionManagement` (Google + Maven Central + Gradle Plugin Portal), `rootProject.name = "pandar-android"`, `include(":app")`.

- [ ] **Step 3: Create root `build.gradle.kts`** declaring the plugins with `apply false`: `com.android.application`, `org.jetbrains.kotlin.android`, `org.jetbrains.kotlin.plugin.compose`, `org.jetbrains.kotlin.plugin.serialization`.

- [ ] **Step 4: Create `gradle.properties`**: `android.useAndroidX=true`, `android.nonTransitiveRClass=true`, `kotlin.code.style=official`, `org.gradle.jvmargs=-Xmx2048m`.

- [ ] **Step 5: Create `gradle/wrapper/gradle-wrapper.properties`** with `distributionUrl=https\://services.gradle.org/distributions/gradle-8.10.2-bin.zip`. Create `gradlew`/`gradlew.bat` (standard 8.10.2 wrapper scripts). The `gradle-wrapper.jar` is a binary; document in `docs/android.md` that the user must run `gradle wrapper` once (or open in Android Studio) if the jar is missing, and add a fallback note. (We will include the jar if obtainable; otherwise document.)

- [ ] **Step 6: Create `app/build.gradle.kts`**: `plugins` apply the four plugins. `android { namespace="zip.iptables.pandar.android"; compileSdk=35; defaultConfig { applicationId="zip.iptables.pandar.android"; minSdk=26; targetSdk=35; versionCode=1; versionName="0.1.0"; testInstrumentationRunner="androidx.test.runner.AndroidJUnitRunner"; vectorDrawables { useSupportLibrary=true } }; buildFeatures { compose=true }; compileOptions { sourceCompatibility=JavaVersion.VERSION_17; targetCompatibility=JavaVersion.VERSION_17 }; kotlinOptions { jvmTarget="17"; freeCompilerArgs = listOf("-opt-in=kotlinx.serialization.ExperimentalSerializationApi") }; buildTypes { release { isMinifyEnabled=false; proguardFiles(...) } }; packaging { resources { excludes += "/META-INF/{AL2.0,LGPL2.1}" } } }`. The `-opt-in=kotlinx.serialization.ExperimentalSerializationApi` compiler arg is REQUIRED because the plan uses `explicitNulls`, `@EncodeDefault`, and `@JsonClassDiscriminator` (all `@ExperimentalSerializationApi`). `dependencies` block uses the catalog: implementation `androidx.core:core-ktx`, platform `androidx.compose:compose-bom`, bundles `compose`, `androidx.lifecycle:lifecycle-runtime-ktx`, `androidx.activity:activity-compose`, `androidx.navigation:navigation-compose`, retrofit/converter/okhttp/logging-interceptor/bom, kotlinx-serialization-json, coroutines, datastore-preferences, appauth, browser. testImplementation junit, coroutines-test, mockwebserver.

- [ ] **Step 7: Create `app/src/main/AndroidManifest.xml`**: `<manifest xmlns:android... package implicit via namespace>`. Permissions: `INTERNET`, `ACCESS_NETWORK_STATE`. `<application android:name=".PandarApplication" android:label="@string/app_name" android:theme="@style/Theme.Pandar" android:allowBackup="true" android:dataExtractionRules="@xml/backup_rules">` with `<activity android:name=".MainActivity" android:exported="true" android:theme="@style/Theme.Pandar">` launcher intent. Add a second `<activity android:name="net.openid.appauth.RedirectUriReceiverActivity" android:exported="true">` with intent-filter for `android:scheme="zip.iptables.pandar.android"` (VIEW/BROWSABLE/DEFAULT) to receive the OIDC redirect.

- [ ] **Step 8: Create resources**: `strings.xml` (`app_name="Pandar"`), `themes.xml` + `values-night/themes.xml` defining `Theme.Pandar` as a Material3 theme parent `android:Theme.Material.Light.NoActionBar`/Dark (or use a minimal parent; the Compose theme controls colors so the XML theme just sets background + status bar), `backup_rules.xml` (empty/full-content rules), adaptive launcher icon (`mipmap-anydpi-v26/ic_launcher.xml` referencing `@drawable/ic_launcher_foreground` + `@color/ic_launcher_background`; `ic_launcher_foreground.xml` a simple vector; `ic_launcher_background.xml` color).

- [ ] **Step 9: Create `mobile/android/.gitignore`** ignoring `local.properties`, `.gradle/`, `build/`, `*.iml`, `.idea/`, `captures/`, `.cxx/`.

- [ ] **Step 10: Verify (static)** — confirm all files exist; confirm `applicationId`, namespace, minSdk 26, compileSdk 35, JDK 17; confirm AppAuth redirect intent-filter present. (Gradle build deferred to Android Studio.)

- [ ] **Step 11: Commit** — `git add mobile/android && git commit -m "feat(android): add gradle module skeleton"`.

---

## Task 2: Domain models, Severity, StatusMeta, Logger + unit tests (TDD anchor)

**Files:**
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/domain/model/Severity.kt`
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/domain/status/StatusMeta.kt`
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/domain/model/Models.kt`
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/core/util/Logger.kt`
- Test: `app/src/test/kotlin/zip/iptables/pandar/android/domain/status/StatusMetaTest.kt`

**Interfaces:**
- Produces: `enum class Severity { CRITICAL, WARNING, SUCCESS, INFO }`; `data class StatusMeta(val severity: Severity, val label: String)`; `fun statusMeta(rawStatus: String): StatusMeta` (lowercases, maps per Global Constraints, unknown→INFO, label = prettified token: replace `_`/`-` with space, capitalize first letter). Domain models: `Printer`, `Job`, `Agent`, `Command`, `PrinterNozzleTemp`, `AmsUnit`, `AmsTray`, `ExternalSpool`, `ActiveTray`, `Materials`, `JobPrint`, `JobArtifact` (pure Kotlin data classes, nullable where the hub is nullable). Also `interface Logger { fun d(t: Throwable? = null, msg: () -> String); fun w(t: Throwable? = null, msg: () -> String); fun e(t: Throwable? = null, msg: () -> String) }` — a tiny abstraction used by later tasks (WS repo in Task 7, repository in Task 9). `Logger` is defined here so it is available before Task 7.

- [ ] **Step 1: Write failing test `StatusMetaTest.kt`** with cases:
  - Each SUCCESS token (online, ok, succeeded, completed, running, printing, ready) → SUCCESS, label prettified ("RUNNING" → "Running").
  - Each WARNING token (warning, queued, sent, acknowledged, connecting, problem, degraded, pending) → WARNING.
  - Each CRITICAL token (failed, offline, unavailable, error, down) → CRITICAL.
  - Unknown token ("flumbus", "", "  ") → INFO and never throws; "" → label "Unknown".
  - Case-insensitive ("OFFLINE" == "Offline" == "offline").
  - Label: "needs_attention" → "Needs attention".

```kotlin
@Test fun success_tokens() = assertEquals(Severity.SUCCESS, statusMeta("running").severity)
// ... etc per Global Constraints token list
@Test fun unknown_does_not_throw() { assertEquals(Severity.INFO, statusMeta("flumbus").severity) }
```

- [ ] **Step 2: Run test to verify it fails** — `./gradlew :app:testDebugUnitTest --tests "zip.iptables.pandar.android.domain.status.StatusMetaTest"` (run in Android Studio). Expected: FAIL (symbols not defined). In this env: confirm test file asserts the mapping table from Global Constraints.

- [ ] **Step 3: Implement `Severity.kt`, `StatusMeta.kt`, `Models.kt`** to satisfy the tests and match hub field names.

- [ ] **Step 4: Run test to verify it passes** — same command. Expected: PASS (in Android Studio).

- [ ] **Step 5: Commit** — `git commit -m "feat(android): add domain models and status severity mapping"`.

---

## Task 3: Network DTOs, decoders, and JSON-shape unit tests

**Files:**
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/data/remote/Json.kt`
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/data/remote/dto/Dtos.kt`
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/data/remote/dto/PrinterMaterialsDto.kt`
- Test: `app/src/test/kotlin/zip/iptables/pandar/android/data/remote/dto/PrinterListDtoTest.kt`
- Test: `app/src/test/kotlin/zip/iptables/pandar/android/data/remote/dto/JobsListDtoTest.kt`
- Test: `app/src/test/kotlin/zip/iptables/pandar/android/data/remote/dto/AgentsListDtoTest.kt`
- Test: `app/src/test/kotlin/zip/iptables/pandar/android/data/remote/dto/PrinterEventsDecoderTest.kt`

**Interfaces:**
- Produces: `@Serializable` DTOs mirroring hub JSON: `PrinterDto`, `PrinterListDto`, `AgentDto`, `AgentsListDto`, `JobDto`, `JobListDto`, `CommandResponseDto`, `PrinterEventCommandDto`, and the sealed `PrinterEventDto` annotated `@JsonClassDiscriminator("type")` (experimental, covered by the project-wide opt-in from Task 1 Step 6) with `@SerialName` variants `PrinterSnapshotEvent`, `JobProgressEvent`, `CommandResultEvent`. `PrinterMaterialsDto` + `AmsUnitDto`/`AmsTrayDto`/`ExternalSpoolDto`/`ActiveTrayDto` per spec §4.4 with all-optional fields. A `domain` extension/mapper `PrinterDto.toDomain(): Printer` etc.

- [ ] **Step 1: Write `PrinterListDtoTest`** parsing a representative `{"printers":[{...full PrinterEventPrinter with materials...}]}` JSON (paste a realistic sample including null `model`, null `materials`, nozzle temps with null fields) into `PrinterListDto` and asserting the domain `Printer` fields. Include a case with `materials:null`.

- [ ] **Step 2: Write `JobsListDtoTest`** parsing `{"jobs":[{...full Job...}]}` (include `print.progress_percent`, `remaining_time_minutes`, terminal + active statuses) and assert domain `Job` mapping.

- [ ] **Step 3: Write `AgentsListDtoTest`** parsing `{"agents":[{id,tenant_id,name,status,created_at}]}`.

- [ ] **Step 4: Write `PrinterEventsDecoderTest`** — three JSON strings, one per event variant (`type` = printer_snapshot/job_progress/command_result), decoded via `Json { ignoreUnknownKeys=true; classDiscriminator="type" }` into `PrinterEventDto`, asserting each lands in the correct branch and the nested object decodes.

- [ ] **Step 5: Implement DTOs** with `@Serializable`, `@SerialName`, nullable fields, and the unified `Json` instance defined in this same task (`data/remote/Json.kt`): `val appJson = Json { ignoreUnknownKeys = true; isLenient = true; encodeDefaults = true; explicitNulls = false }`. This single instance is used for BOTH decoding hub responses/events (lenient on unknown keys) and encoding outgoing control request bodies (emit defaults like `action`, omit nulls). Add `toDomain()` mappers. AMS DTOs: numbers-as-strings handled by accepting `JsonElement` or by a custom lenient accessor (simplest: declare `remaining_estimate` etc. as `JsonElement?` and expose typed accessors; for `global_tray_id` use a `@Serializable(with=LenientIntSerializer)` that accepts number-or-string). Provide `LenientIntSerializer` and `LenientStringSerializer` helpers.
- [ ] **Step 6: Create `data/remote/Json.kt`** with exactly: `package zip.iptables.pandar.android.data.remote` / `import kotlinx.serialization.json.Json` / `val appJson: Json = Json { ignoreUnknownKeys = true; isLenient = true; encodeDefaults = true; explicitNulls = false }`. (`explicitNulls` is `@ExperimentalSerializationApi`; the project-wide `-opt-in=kotlinx.serialization.ExperimentalSerializationApi` compiler arg is added in Task 1 Step 6, so no per-file `@OptIn` is needed.)

- [ ] **Step 7: Run tests** — `./gradlew :app:testDebugUnitTest`. Expected: PASS (Android Studio). In env: static review of expected values vs. hub DTO.

- [ ] **Step 8: Commit** — `git commit -m "feat(android): add hub DTOs, event decoder, and json-shape tests"`.

---

## Task 4: Control request body shapes + strict-parity unit tests

**Files:**
- Modify: `app/src/main/kotlin/zip/iptables/pandar/android/data/remote/dto/Dtos.kt` (add request bodies) — or new file `data/remote/dto/ControlRequests.kt`
- Test: `app/src/test/kotlin/zip/iptables/pandar/android/data/remote/ControlsBodyShapeTest.kt`

**Interfaces:**
- Produces: `@Serializable` request bodies, one per implemented action, each a CONCRETE top-level class (NOT a sealed-interface/polymorphic hierarchy — see Task 5 serialization note). Each class declares `@SerialName("action") @EncodeDefault val action: String = "<literal>"` so the `action` key is always emitted regardless of the `Json` configuration, plus the per-action required fields and nullable optional fields with `@SerialName` snake_case names. Optional nullable fields are omitted from output via `explicitNulls=false`. Types: `PauseRequest`, `ResumeRequest`, `StopRequest`, `ToggleLightRequest`, `SetChamberLightRequest(on:Boolean)`, `SetHotendTemperatureRequest(temperatureCelsius:Int, wait:Boolean, extruderId:Int?=null)`, `SetBedTemperatureRequest(temperatureCelsius:Int, wait:Boolean)`, `SetChamberTemperatureRequest(temperatureCelsius:Int, wait:Boolean)`, `AmsRereadRfidRequest(amsId:Int, slotId:Int)`, `AmsLoadFilamentRequest(amsId:Int, slotId:Int, globalTrayId:Int?=null, externalId:String?=null, extruderId:Int?=null)`, `AmsUnloadFilamentRequest(amsId:Int, slotId:Int, globalTrayId:Int?=null, externalId:String?=null, extruderId:Int?=null)`. Field `@SerialName`s: `temperature_celsius`, `wait`, `extruder_id`, `light_on`, `ams_id`, `slot_id`, `global_tray_id`, `external_id`.

- [ ] **Step 1: Write `ControlsBodyShapeTest`** using the SAME production `Json` instance the Retrofit converter uses (`appJson` from `data/remote/Json.kt`, created in Task 3), NOT a default `Json`. Each assertion checks the EXACT serialized JSON string: correct `action` literal, no extra keys, no `type` discriminator key, optional nulls omitted.

```kotlin
private val json = appJson  // the EXACT instance Retrofit uses for both encode and decode

@Test fun pause_is_minimal() =
  assertEquals("""{"action":"pause"}""", json.encodeToString(PauseRequest()))
@Test fun no_polymorphic_discriminator_leaks() {
  val s = json.encodeToString(PauseRequest())
  assertFalse("type discriminator leaked", s.contains("\"type\""))
}
@Test fun set_bed_temperature() =
  assertEquals("""{"action":"set_bed_temperature","temperature_celsius":60,"wait":false}""", json.encodeToString(SetBedTemperatureRequest(60,false)))
@Test fun set_hotend_temperature_omits_null_extruder() =
  assertEquals("""{"action":"set_hotend_temperature","temperature_celsius":220,"wait":true}""", json.encodeToString(SetHotendTemperatureRequest(220,true,null)))
@Test fun set_hotend_temperature_with_extruder() =
  assertEquals("""{"action":"set_hotend_temperature","temperature_celsius":220,"wait":true,"extruder_id":0}""", json.encodeToString(SetHotendTemperatureRequest(220,true,0)))
@Test fun ams_load_minimal() =
  assertEquals("""{"action":"ams_load_filament","ams_id":1,"slot_id":2}""", json.encodeToString(AmsLoadFilamentRequest(1,2)))
@Test fun ams_load_global_tray() =
  assertEquals("""{"action":"ams_load_filament","ams_id":1,"slot_id":2,"global_tray_id":5}""", json.encodeToString(AmsLoadFilamentRequest(1,2,globalTrayId=5)))
@Test fun ams_load_external_id() =
  assertEquals("""{"action":"ams_load_filament","ams_id":1,"slot_id":2,"external_id":"ext1"}""", json.encodeToString(AmsLoadFilamentRequest(1,2,externalId="ext1")))
@Test fun ams_unload_extruder() =
  assertEquals("""{"action":"ams_unload_filament","ams_id":1,"slot_id":2,"extruder_id":0}""", json.encodeToString(AmsUnloadFilamentRequest(1,2,extruderId=0)))
// + resume, stop, toggle_light, set_chamber_light, set_chamber_temperature, ams_reread_rfid, ams_unload (minimal) — each one assertion with the exact expected string
```

- [ ] **Step 2: Run test → FAIL** (symbols undefined).

- [ ] **Step 3: Implement `ControlRequests.kt`** using one concrete `@Serializable data class` per action. Each declares `@SerialName("action") @EncodeDefault val action: String = "<literal>"` (`@EncodeDefault` is `@ExperimentalSerializationApi`, covered by the project-wide opt-in from Task 1 Step 6; combined with `appJson.encodeDefaults=true` from Task 3 this is belt-and-suspenders). Optional nullable fields use `@SerialName("...")` and are omitted when null because `explicitNulls=false`. Do NOT make these classes implement any common sealed/interface supertype that kotlinx.serialization would treat as polymorphic (no `sealed interface ControlRequest`).

- [ ] **Step 4: Run test → PASS** (Android Studio). The `no_polymorphic_discriminator_leaks` test MUST pass, proving the production encoder emits no `type` key. In env: assert expected JSON matches the "Control actions + exact key sets" table in this plan exactly and that no assertion references a `type` key.

- [ ] **Step 5: Commit** — `git commit -m "feat(android): add printer control request bodies with strict shape tests"`.

---

## Task 5: ApiModule (Json, OkHttp, Retrofit), BearerAuthInterceptor, PandarApi interface

**Files:**
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/data/remote/ApiModule.kt`
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/data/remote/BearerAuthInterceptor.kt`
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/data/remote/PandarApi.kt`

**Interfaces:**
- Consumes: `SettingsRepository.currentToken(): String?` via the `TokenProvider` interface (defined here in Task 5; `SettingsRepository` implements it in Task 6).
- Produces: `interface TokenProvider { fun currentToken(): String? }`; `class BearerAuthInterceptor(private val tokenProvider: TokenProvider) : Interceptor` adds `Authorization: Bearer <token>` when token non-null; `object ApiModule` builds `OkHttpClient` (timeouts 30s connect/read/write, +interceptor + HttpLoggingInterceptor on debug) and `Retrofit` with the kotlinx-serialization converter built from the unified `appJson` (created in Task 3) — `appJson` is safe for BOTH directions: `ignoreUnknownKeys`/`isLenient` for response decoding and `encodeDefaults`/`explicitNulls=false` for request encoding. `interface PandarApi` with suspend endpoints matching the REST list above. The Retrofit `Json` converter must serialize the concrete request class, NOT a polymorphic base type (see serialization note).

- [ ] **Step 1: Implement `TokenProvider`, `BearerAuthInterceptor`** (skips when token null → no-auth hub support).

- [ ] **Step 2: Implement `PandarApi`** with ONE concrete-typed `@Body` parameter per control action so kotlinx.serialization serializes the concrete class directly (no sealed interface, no polymorphic discriminator). Use a single private backend method if desired, but expose them as separate interface methods:
```kotlin
interface PandarApi {
  @GET("api/v1/tenants/{tenant}/printers")        suspend fun listPrinters(@Path("tenant") t:String): PrinterListDto
  @GET("api/v1/tenants/{tenant}/printers/{printer}") suspend fun getPrinter(@Path("tenant") t:String, @Path("printer") p:String): PrinterDto
  @GET("api/v1/tenants/{tenant}/agents")          suspend fun listAgents(@Path("tenant") t:String): AgentsListDto
  @GET("api/v1/tenants/{tenant}/jobs")            suspend fun listJobs(@Path("tenant") t:String): JobListDto
  @POST("api/v1/tenants/{tenant}/printers/{printer}/controls") suspend fun pause(@Path("tenant") t:String, @Path("printer") p:String, @Body body: PauseRequest): CommandResponseDto
  @POST("api/v1/tenants/{tenant}/printers/{printer}/controls") suspend fun resume(@Path("tenant") t:String, @Path("printer") p:String, @Body body: ResumeRequest): CommandResponseDto
  @POST("api/v1/tenants/{tenant}/printers/{printer}/controls") suspend fun stop(@Path("tenant") t:String, @Path("printer") p:String, @Body body: StopRequest): CommandResponseDto
  @POST("api/v1/tenants/{tenant}/printers/{printer}/controls") suspend fun toggleLight(@Path("tenant") t:String, @Path("printer") p:String, @Body body: ToggleLightRequest): CommandResponseDto
  @POST("api/v1/tenants/{tenant}/printers/{printer}/controls") suspend fun setChamberLight(@Path("tenant") t:String, @Path("printer") p:String, @Body body: SetChamberLightRequest): CommandResponseDto
  @POST("api/v1/tenants/{tenant}/printers/{printer}/controls") suspend fun setHotendTemperature(@Path("tenant") t:String, @Path("printer") p:String, @Body body: SetHotendTemperatureRequest): CommandResponseDto
  @POST("api/v1/tenants/{tenant}/printers/{printer}/controls") suspend fun setBedTemperature(@Path("tenant") t:String, @Path("printer") p:String, @Body body: SetBedTemperatureRequest): CommandResponseDto
  @POST("api/v1/tenants/{tenant}/printers/{printer}/controls") suspend fun setChamberTemperature(@Path("tenant") t:String, @Path("printer") p:String, @Body body: SetChamberTemperatureRequest): CommandResponseDto
  @POST("api/v1/tenants/{tenant}/printers/{printer}/controls") suspend fun amsRereadRfid(@Path("tenant") t:String, @Path("printer") p:String, @Body body: AmsRereadRfidRequest): CommandResponseDto
  @POST("api/v1/tenants/{tenant}/printers/{printer}/controls") suspend fun amsLoadFilament(@Path("tenant") t:String, @Path("printer") p:String, @Body body: AmsLoadFilamentRequest): CommandResponseDto
  @POST("api/v1/tenants/{tenant}/printers/{printer}/controls") suspend fun amsUnloadFilament(@Path("tenant") t:String, @Path("printer") p:String, @Body body: AmsUnloadFilamentRequest): CommandResponseDto
  @POST("api/v1/tenants/{tenant}/jobs/{job}/retry-dispatch") suspend fun retryDispatch(@Path("tenant") t:String, @Path("job") j:String): CommandResponseDto
  @POST("api/v1/tenants/{tenant}/jobs/{job}/reprint")       suspend fun reprint(@Path("tenant") t:String, @Path("job") j:String): CommandResponseDto
}
```
Serialization note: because each `@Body` is a concrete class, the kotlinx-serialization converter emits exactly that class's fields. The `no_polymorphic_discriminator_leaks` test in Task 4 guards this. (Retrofit allows multiple methods to share the same request line; the converter picks the serializer from the concrete `@Body` type.)

- [ ] **Step 3: Implement `ApiModule`** — expose `okHttp(tokenProvider)`, `retrofit(baseUrl, client)` using the kotlinx-serialization-converter built from `appJson` (`appJson.asConverterFactory("application/json".toMediaType())`), and `pandarApi(...)`. Build base URL from the user hub URL (append `/`).

- [ ] **Step 4: Verify (static)** — confirm interceptor path, Retrofit baseUrl construction, endpoint paths match the contract list exactly, and that the converter is constructed from the SAME `appJson` instance `ControlsBodyShapeTest` uses.

- [ ] **Step 5: Commit** — `git commit -m "feat(android): add retrofit/okhttp api module and bearer auth"`.

---

## Task 6: SettingsRepository (DataStore) + mapping test

> Ordered before the WebSocket repository because the WS repo consumes `SettingsRepository.tenantId`.

**Files:**
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/data/settings/SettingsSnapshot.kt`
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/data/settings/SettingsRepository.kt`
- Test: `app/src/test/kotlin/zip/iptables/pandar/android/data/settings/SettingsMappingTest.kt`

**Interfaces:**
- Consumes: `TokenProvider` (defined Task 5).
- Produces: `data class SettingsSnapshot(hubBaseUrl:String?, tenantId:String?, oidcDiscoveryUrl:String?, oidcClientId:String?, oidcScopes:String?, oidcRedirectUri:String?, accessToken:String?, refreshToken:String?, tokenExpiresAtEpochMillis:Long?)`; `class SettingsRepository(context, scope): TokenProvider` with `val settings: Flow<SettingsSnapshot>`, a convenience `val tenantId: Flow<String?>` (derived from `settings.map { it.tenantId }` — consumed by Task 7 WS repo), `suspend fun update(transform:(SettingsSnapshot)->SettingsSnapshot)`, `override fun currentToken(): String?`, `suspend fun setTokens(access:String?, refresh:String?, expiresAtMillis:Long?)`, `suspend fun clearTokens()`. Implements `TokenProvider`.

- [ ] **Step 1: Write `SettingsMappingTest`** — a pure test of the (extracted) `settingsToSnapshot(map)` / `snapshotToUpdates(...)` mapping functions covering null vs set keys, defaults, and the scopes-string round trip (comma-join/split). This avoids needing Robolectric/DataStore in JVM tests by extracting the pure mapping logic.

- [ ] **Step 2: Run → FAIL** (Android Studio) / static review in env.

- [ ] **Step 3: Implement** `SettingsRepository` using `preferencesDataStore`, key constants, `map { it.toSettingsSnapshot() }`, the `tenantId` derived Flow, and the pure mappers under test.

- [ ] **Step 4: Run → PASS** (Android Studio).

- [ ] **Step 5: Commit** — `git commit -m "feat(android): add datastore settings repository"`.

---

## Task 7: PrinterEventsRepository (WebSocket) + auth-refresh interaction

**Files:**
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/data/remote/ws/PrinterEventsRepository.kt`

**Interfaces:**
- Consumes: `OkHttpClient`, `appJson` (Task 3), `SettingsRepository.tenantId: Flow<String?>` (now available from Task 6) and `SettingsRepository.settings: Flow<SettingsSnapshot>` (for the current hub base URL), `TokenProvider` (Task 5/6), a `tokenRefresher: suspend () -> Boolean` (from AuthRepository Task 8), `Logger`.
- Produces: `class PrinterEventsRepository` with `val events: SharedFlow<PrinterEventDto>` and `fun start(scope: CoroutineScope)`, `fun stop()`. Maintains connection state `StateFlow<LiveState>` where `enum LiveState { CONNECTED, CONNECTING, DISCONNECTED }`.

- [ ] **Step 1: Implement** a `connectLoop` that: builds the WS URL `ws(s)://<hub>/api/v1/tenants/<tenant>/printer-events` (hub base from `SettingsSnapshot.hubBaseUrl`, tenant from `tenantId`), opens with OkHttp `newWebSocket(Request.Builder().url(url).addHeader("Authorization","Bearer <token>").build(), listener)`. On `onMessage(text)` decode via `appJson` into `PrinterEventDto` and emit. On `onFailure`/`onClosed`: if the failure indicates auth (HTTP 401/403 handshake — OkHttp surfaces via `response.code` in `onClosed` for HTTP-rejected upgrades, or `Exception` containing 401), call `tokenRefresher()` once; if it returns true, reset backoff and reconnect; else emit a re-sign-in signal via a separate `StateFlow<Boolean> needsReauth`. Otherwise exponential backoff 1s..30s capped.

- [ ] **Step 2: Verify (static)** — confirm: header name `Authorization`, value prefix `Bearer `, URL path matches, backoff bounds, single-refresh path, re-sign-in signal.

- [ ] **Step 3: Commit** — `git commit -m "feat(android): add printer-events websocket repository"`.

---

## Task 8: AuthRepository (AppAuth OIDC) + LoginViewModel + LoginScreen

**Files:**
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/data/auth/AuthRepository.kt`
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/data/auth/AuthEvent.kt`
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/ui/login/LoginViewModel.kt`
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/ui/login/LoginScreen.kt`

**Interfaces:**
- Consumes: `SettingsSnapshot` (OIDC config), `SettingsRepository`, Android `Context` (for `AuthorizationServicesConfiguration` and CustomTabIntent), Activity result from the redirect.
- Produces: `class AuthRepository(context, settings, coroutineScope)` with `val state: StateFlow<AuthState>` (`enum AuthState { SIGNED_OUT, SIGNING_IN, SIGNED_IN, NEEDS_CONFIG }`), `suspend fun signIn(): AuthIntent` (returns the Custom Tab intent + the auth request), `suspend fun handleAuthorizationResponse(intent: Intent)`, `suspend fun refresh(): Boolean`, `fun signOut()`. `AuthEvent` sealed for UI (`ShowToast`, `LaunchBrowser(intent)`).

- [ ] **Step 1: Implement `AuthRepository`** using AppAuth-Android's built-in PKCE flow: build `AuthorizationServiceConfiguration.fetchFromUrl(discoveryUrl, listener)` (suspend-bridge via callback adapter). Construct `AuthorizationRequest.Builder(config, clientId, ResponseTypeValues.CODE, redirectUri).setScopes(scopes).build()` — AppAuth's default `AuthorizationRequest` already enables PKCE S256: it generates the code verifier via `CodeVerifierUtil.generateRandomCodeVerifier()` (an AppAuth utility, not a non-existent `CodeChallengeUtil`) and sets `code_challenge`/`code_challenge_method` on the request automatically; do NOT pass a manual `setCodeVerifier` unless overriding. Launch via `AuthorizationService(context).performAuthorizationRequest(request, postAuthPendingIntent)` (preferred) or build a Custom Tab intent via `AuthorizationService.createCustomTabIntentBuilder(request)` → emit `LaunchBrowser(intent)`. On `handleAuthorizationResponse(intent)`: extract `AuthorizationResponse.fromIntent(intent)` and `AuthorizationException.fromIntent(intent)`; build `resp.createTokenExchangeRequest()` (carries the PKCE verifier) and call `AuthorizationService(context).performTokenRequest(tokenRequest, callback)` with grant_type=authorization_code (AppAuth sets this); persist `accessToken`, `refreshToken`, and `accessTokenExpirationTime` into `SettingsRepository` via `setTokens(...)`. `refresh()`: if a `refreshToken` exists, build `TokenRequest.Builder(config, clientId).setGrantType(GrantTypeValues.REFRESH_TOKEN).setRefreshToken(refreshToken).build()` and perform; return success; on failure clear tokens and return false. `signOut()`: clear tokens; if the discovery doc has an `endSessionEndpoint`, optionally build an `EndSessionRequest` and launch it, but discard tokens regardless.

- [ ] **Step 2: Implement `LoginViewModel`** exposing `uiState`, `fun signIn()`, consuming `AuthRepository.state` to drive navigation. `LoginScreen` — short copy + "Sign in" button + error text.

- [ ] **Step 3: Verify (static)** — confirm PKCE is enabled by default via AppAuth's `AuthorizationRequest` + `CodeVerifierUtil` (no manual challenge util), response_type code, correct grant types (authorization_code on exchange, refresh_token on refresh), token storage keys match SettingsRepository, redirect scheme matches manifest intent-filter.

- [ ] **Step 4: Commit** — `git commit -m "feat(android): add appauth oidc login flow"`.

---

## Task 9: PandarRepository (REST orchestration) + AppContainer + PandarApplication

**Files:**
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/data/repository/PandarRepository.kt`
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/core/di/AppContainer.kt`
- Create: `app/src/main/kotlin/zip/iptables/pandar/android/PandarApplication.kt`
- Modify: `app/src/main/kotlin/zip/iptables/pandar/android/data/remote/ApiModule.kt` (wire AppContainer usage)

**Interfaces:**
- Produces: `class PandarRepository(api, settings, ws, logger: Logger)` (`Logger` interface already defined in Task 2) exposing `suspend fun printers(): List<Printer>`, `suspend fun printer(id): Printer`, `suspend fun agents(): List<Agent>`, `suspend fun jobs(): List<Job>`, one typed `suspend fun control(...)` method per concrete request class from Task 4 delegating to the matching `PandarApi` method (e.g. `suspend fun pause(tenant, printerId): Command`, `suspend fun setHotendTemperature(tenant, printerId, body: SetHotendTemperatureRequest): Command`, … — 12 methods total, each calling the same-named `PandarApi` method), `suspend fun retry(tenant, jobId): Command`, `suspend fun reprint(tenant, jobId): Command`, plus `val events: Flow<PrinterEventDto>`, `val liveState: StateFlow<LiveState>`, `val needsReauth: StateFlow<Boolean>`. `class AppContainer(context)` constructs everything lazily: `settings`, `auth`, `apiModule` (depends on settings base URL — rebuild on base-URL change), `ws`, `pandar`, plus a `Logger` implementation. `PandarApplication` implements `Application`, creates `container`, exposes it.

- [ ] **Step 1: Implement `PandarRepository`, `AppContainer`, `PandarApplication`** (the `Logger` interface already exists from Task 2; here you add a concrete `AndroidLogger` implementation inside `AppContainer`). AppContainer must rebuild Retrofit when `hubBaseUrl` changes (observe settings; recreate api on change). Provide a `fun tokenProvider()` returning settings.

- [ ] **Step 2: Verify (static)** — DI wiring, base-URL rebuild, no Hilt.

- [ ] **Step 3: Commit** — `git commit -m "feat(android): add repository, appcontainer di, and application"`.

---

## Task 10: Theme (Color/Type/Theme) + reusable components (StatusPill, FormFields)

**Files:**
- Create: `ui/theme/Color.kt`, `ui/theme/Type.kt`, `ui/theme/Theme.kt`
- Create: `ui/components/StatusPill.kt`, `ui/components/FormFields.kt`

**Interfaces:**
- Produces: Material3 `lightColorScheme`/`darkColorScheme` with neutral palette from DESIGN.md (background white/oklch(0.145), foreground inverse; primary near-black/white; surface white/dark; error red). `PandarTheme(darkTheme, dynamicColor=false, content)`. Type: default sans + a `MonoFontFamily` (`FontFamily.Monospace`) used by `MonoText`. `StatusPill(rawStatus)` renders icon+label colored by severity (icons: Success=check, Warning=warning, Critical=error, Info=info). `MonoText(text)` composable. `FormFields`: `LabeledTextField`, `PrimaryButton`.

- [ ] **Step 1: Implement** the four files. StatusPill uses `MaterialTheme.colorScheme` semantic containers (e.g. for critical use a red-tinted container; pair with icon + label).

- [ ] **Step 2: Verify (static)** — color + icon + label present; never color alone.

- [ ] **Step 3: Commit** — `git commit -m "feat(android): add material3 theme and shared components"`.

---

## Task 11: Navigation graph + MainActivity + Settings screen

**Files:**
- Create: `ui/navigation/PandarNavGraph.kt`
- Create: `ui/settings/SettingsScreen.kt`, `ui/settings/SettingsViewModel.kt`
- Create: `MainActivity.kt`

**Interfaces:**
- Produces: `PandarNavGraph(navController, container)` with routes `login`, `printers`, `printers/{printerId}`, `jobs`, `settings`. Bottom nav with Printers/Jobs/Settings. Start destination = printers if authenticated & configured, else login. `SettingsViewModel(container)` exposes `state: SettingsState` (current snapshot + auth state), `update{...}`, `signIn()`, `signOut()`. `MainActivity` sets Compose content + `PandarTheme`.

- [ ] **Step 1: Implement `SettingsScreen`** — form fields bound to SettingsViewModel; Save button persists; Sign in/out buttons; shows identity when available.

- [ ] **Step 2: Implement `PandarNavGraph` + `MainActivity`** wiring ViewModels via a `viewModelFactory { initializer { ...container... } }`.

- [ ] **Step 3: Verify (static)** — routes, start-destination logic, ViewModel wiring.

- [ ] **Step 4: Commit** — `git commit -m "feat(android): add navigation graph, main activity, and settings screen"`.

---

## Task 12: Printers screen (dashboard) + PrinterCard + PrintersViewModel

**Files:**
- Create: `ui/printers/PrintersScreen.kt`, `ui/printers/PrintersViewModel.kt`, `ui/printers/PrinterCard.kt`

**Interfaces:**
- Consumes: `PandarRepository` (printers, agents, events).
- Produces: `PrintersViewModel` exposing `state: PrintersUiState { loading, printers:List<Printer>, agents:List<Agent>, liveState, error }`. Loads via REST on init + fold WS `printer_snapshot` into the printers list (replace matching id) and `command_result` to nudge refresh. `PrintersScreen` renders a summary strip (total printers, online printers, connected agents) + LazyColumn of `PrinterCard`. Pull-to-refresh triggers REST reload + WS reconnect-if-down. Card tap navigates to `printers/{id}`.

- [ ] **Step 1: Implement `PrinterCard`** — status pill, name, model, serial (MonoText), bed current/target, active hotend current/target, chamber-light indicator (icon). Online count = printers whose `statusMeta(status).severity != CRITICAL`.

- [ ] **Step 2: Implement `PrintersViewModel`** — `init { load() }`, collect WS events into state, expose `refresh()`.

- [ ] **Step 3: Implement `PrintersScreen`** — Scaffold top bar + pull-to-refresh (`androidx.compose.material3.pulltorefresh`) + content.

- [ ] **Step 4: Verify (static)** — WS folding by id, summary counts, offline detection uses severity table.

- [ ] **Step 5: Commit** — `git commit -m "feat(android): add printers dashboard screen"`.

---

## Task 13: Printer detail screen + ViewModel (controls)

**Files:**
- Create: `ui/printerdetail/PrinterDetailScreen.kt`, `ui/printerdetail/PrinterDetailViewModel.kt`

**Interfaces:**
- Consumes: `PandarRepository` (printer, control, events).
- Produces: `PrinterDetailViewModel(printerId)` exposing `state` (printer, materials, command-in-flight, lastCommand status, error) and one intent function per control that builds the matching concrete request and calls the same-named `PandarRepository` control method: `pause()`, `resume()`, `stop()`, `toggleLight()`, `setChamberLight(on)`, `setHotend(temp, wait, extruderId?)`, `setBed(temp, wait)`, `setChamber(temp, wait)`, `amsReread(amsId, slotId)`, `amsLoad(request: AmsLoadFilamentRequest)`, `amsUnload(request: AmsUnloadFilamentRequest)`. The UI builds the load/unload request from the tray per spec §4.4: required `amsId`←`unit_id` (parsed Int) and `slotId`←`tray_id` (parsed Int); optional `globalTrayId`←`global_tray_id` (when present & parseable), `externalId`←`external_id` (external spools only, when present), `extruderId` only when the user explicitly selects one. Each surfaces `CommandResponseDto` status. WS `command_result` for the printer updates last-known status.

- [ ] **Step 1: Implement the ViewModel** with a per-action `sendControl(coroutine)` pattern that toggles `inFlight`, calls the matching repository method, catches errors, and emits snackbar events. Because each control maps to one concrete request type and one repository method, there is no shared `ControlRequest` supertype in the call path.

- [ ] **Step 2: Implement `PrinterDetailScreen`** — sections: Status header; Nozzles list; Bed/Chamber current vs target with editable temperature fields + Apply buttons (numeric Int, defaulting to current target); Pause/Resume/Stop row; Chamber light switch (uses `chamber_light_on` initial state, calls `set_chamber_light`); AMS section listing `ams_units[].trays[]` with color swatch + hex label + type + remaining, and per-tray Load/Unload/Reread buttons (disabled when ams_id/slot_id unparseable, with caption); empty state when `materials == null`. External spools section likewise with external_id-based actions disabled appropriately.

- [ ] **Step 3: Verify (static)** — each control calls the correct body ctor from Task 4; AMS id coercion (parse tray_id/unit_id to Int; disable on failure); color always paired with label.

- [ ] **Step 4: Commit** — `git commit -m "feat(android): add printer detail screen with temp and ams controls"`.

---

## Task 14: Jobs screen + ViewModel

**Files:**
- Create: `ui/jobs/JobsScreen.kt`, `ui/jobs/JobsViewModel.kt`

**Interfaces:**
- Consumes: `PandarRepository` (jobs, events, retry, reprint).
- Produces: `JobsViewModel` exposing `state` (loading, jobs, error) and `retry(jobId)`, `reprint(jobId)`. Loads REST on init; folds WS `job_progress` into the list (replace matching id).

- [ ] **Step 1: Implement `JobsViewModel`.**

- [ ] **Step 2: Implement `JobsScreen`** — LazyColumn of jobs: filename (artifact.filename), status pill, progress %, remaining time (format `remaining_time_minutes` as `Xh Ym`), layer current/total, timestamps. Retry/Reprint buttons (disabled in-flight and when terminal-with-no-retry; surface command status via snackbar).

- [ ] **Step 3: Verify (static)** — retry/reprint call the right endpoints; button-disabled logic.

- [ ] **Step 4: Commit** — `git commit -m "feat(android): add jobs screen with retry and reprint"`.

---

## Task 15: Docs + roadmap + README + root .gitignore

**Files:**
- Create: `docs/android.md`
- Modify: `docs/roadmap.md` (append completed entry under the appropriate section)
- Modify: `README.md` (add a `mobile/android` pointer in the Workspace section)
- Modify: `/.gitignore` (root) — add `/mobile/android/local.properties`, `/mobile/android/.gradle/`, `/mobile/android/build/`, `/mobile/android/app/build/`, `/mobile/android/captures/`, `/mobile/android/.idea/`, `/mobile/android/*.iml` (this is a docs/config edit, allowed; AC9 covers Rust/TS only).

- [ ] **Step 1: Write `docs/android.md`** — prerequisites (Android Studio Ladybug+, JDK 17, Android SDK 35), how to open `mobile/android`, run `./gradlew :app:testDebugUnitTest`, build `./gradlew :app:assembleDebug`; how to configure hub base URL + tenant id + OIDC discovery/clientId/scopes/redirect in the Settings screen; the AppAuth redirect scheme `zip.iptables.pandar.android`; note about `gradle-wrapper.jar` fallback (`gradle wrapper`) if missing; reference the design spec path.

- [ ] **Step 2: Update `docs/roadmap.md`** — add a "Completed" bullet: "Added a Jetpack Compose + Material 3 Android app under `mobile/android/` (package `zip.iptables.pandar.android`) that monitors printers/jobs and controls Bambu machines via the pandar-hub HTTP/WebSocket API, authenticated with an external OIDC provider via AppAuth + PKCE."

- [ ] **Step 3: Update `README.md` Workspace section** — add `- \`mobile/android\` - Jetpack Compose Android app.`

- [ ] **Step 4: Update root `.gitignore`.**

- [ ] **Step 5: Commit** — `git commit -m "docs(android): add android build/setup docs and roadmap entry"`.

---

## Final verification (in-env)

- [ ] **V1:** `rg -n "applicationId|namespace|minSdk|compileSdk|JavaVersion.VERSION_17" mobile/android/app/build.gradle.kts` → confirm values.
- [ ] **V2:** Confirm no Rust/TS files changed: `git diff --name-only main..HEAD -- crates/ frontend/` → empty (or only the docs/README/.gitignore allowed set). Compare against base.
- [ ] **V3:** Confirm test files exist and reference the Global-Constraints mapping table: `ls mobile/android/app/src/test/kotlin/zip/iptables/pandar/android/...`.
- [ ] **V4 (deferred to Android Studio, documented):** `cd mobile/android && ./gradlew :app:testDebugUnitTest` and `./gradlew :app:assembleDebug`.
- [ ] **V5 (must pass in-env):** `cargo fmt`, `cargo clippy --workspace`, `cargo nextest run --workspace`, `npm run build:web` — unaffected (Rust/TS untouched). (Run if the repo's CI gates matter; otherwise confirm no edits.)

## Self-review notes

- Spec coverage: AC1 (Task 1 + 15), AC2 (Task 12), AC3 (Task 13), AC4 (Task 14), AC5 (Task 7 + folding in 12/14), AC6 (Task 8), AC7 (Tasks 2, 3, 4, 6), AC8 (Task 10), AC9 (no Rust/TS edits; only docs + .gitignore). §4.4 AMS mapping covered in Tasks 3 (DTO), 13 (UI + coercion). Retry/reprint routes in Task 3 (DTO/endpoint) + 14 (UI). WS auth-refresh in Task 7.
- Placeholder scan: none; every step names exact files, code shapes, and verification.
- Type consistency: `TokenProvider` defined in Task 5, implemented in Task 6 (`SettingsRepository`), consumed in Tasks 7/9. The unified `appJson` is defined in Task 3 (`data/remote/Json.kt`) and consumed by Task 3 DTO decoders, Task 4 `ControlsBodyShapeTest`, Task 7 WS decoder, and Task 5's Retrofit converter — no forward dependency (Task 4 test can compile/pass once Task 3 lands, before Task 5). Concrete control request classes defined in Task 4, each used as a concrete `@Body` in one `PandarApi` method in Task 5, exposed as one typed `PandarRepository.control(...)` in Task 9, and built+sent in Task 13 (no shared polymorphic supertype). `PrinterEventDto` decoded in Task 3 (via `appJson`), consumed in Tasks 7/9/12/14. `Severity`/`statusMeta` defined Task 2, consumed Tasks 10/12/13.
