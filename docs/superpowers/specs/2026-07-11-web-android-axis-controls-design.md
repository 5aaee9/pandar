# Web and Android XYZ Axis Controls Design

**Date:** 2026-07-11

## Goal

Add user-facing X, Y, and Z relative movement plus full-axis Homing to the Pandar Web and Android printer detail UIs. The clients use the existing tenant printer-control endpoint and preserve the behavior already implemented for semantic Home and MoveAxes operations.

## Scope

This change includes:

- Web controls for X, Y, and Z moves of `-10`, `-1`, `+1`, and `+10` mm.
- Android controls for the same axis and distance combinations.
- A full-axis Home action in both clients.
- A confirmation step for Home, matching Bambu Studio's auto-homing interaction.
- Exact typed or structured request bodies for the existing Hub endpoint.
- Web English and Chinese translations.
- Android API, repository, ViewModel, navigation, and Compose wiring.
- Tests for the Web request bodies and UI, and Android serialized request bodies.
- Updates to `docs/android.md` and `docs/roadmap.md`.

This change does not include:

- Hub, Agent, protobuf, MQTT, database, or network-plugin behavior changes.
- Per-axis Homing.
- Arbitrary movement distances or user-entered feedrates.
- Displaying or editing the printer's current coordinates.
- Live movement or Homing against physical hardware during automated verification.

## Reference Behavior

Bambu Studio exposes 1 mm and 10 mm axis movements and a full auto-home action. Its legacy movement path uses 3000 mm/min for X/Y and 900 mm/min for Z, and it asks for confirmation before auto-homing.

Pandar already accepts these tenant control requests:

```json
{
  "action": "home",
  "axes": []
}
```

```json
{
  "action": "move_axes",
  "movements": [
    {
      "axis": "x",
      "delta_mm": 10
    }
  ],
  "feedrate_mm_per_min": 3000
}
```

Both clients send one axis movement per request. X and Y requests use `feedrate_mm_per_min: 3000`; Z requests use `feedrate_mm_per_min: 900`. They do not send `required_device_features`. Device-specific transport selection remains behind the existing semantic printer-operation boundary.

## Considered UI Approaches

### 1. Explicit signed buttons per axis (selected)

Each axis has four buttons: `-10`, `-1`, `+1`, and `+10` mm. The action is visible before the user clicks it, there is no local step-size state, and every button maps to one exact API request. This is the smallest UI that exposes all Studio movement increments without hiding direction or adding custom input.

### 2. Step selector plus directional controls

A 1/10 mm selector would reduce the number of visible buttons, but it adds mutable UI state and makes the submitted distance depend on a prior selection. It is less explicit and requires more interaction.

### 3. Arbitrary numeric displacement

A numeric input offers more flexibility but exposes values that Studio does not offer, requires additional validation, and increases the risk of unintended movement. It is out of scope.

## Web Design

### Component placement

Create `frontend/app/dashboard-printer-axis-controls.tsx` and render it from `PrinterCard` after the existing general printer controls. Keeping the component separate avoids pushing `dashboard-printer-temperature-controls.tsx`, currently close to the repository's 400-line production-module limit, beyond that limit.

The component has a full-width `Move axes` dialog trigger. The dialog contains:

- a short movement warning/description;
- one row each for X, Y, and Z;
- four signed movement buttons in each row;
- a distinct `Home all axes` action;
- a confirmation dialog before submitting Home.

The axis buttons are normal forms targeting the existing `controlPrinter` server action. Each form includes `tenant_id`, `printer_id`, `action=move_axes`, `axis`, `delta_mm`, and the axis-specific `feedrate_mm_per_min`. The Home form includes `action=home` and is submitted only after confirmation.

The component does not inspect printer status, model, current coordinates, Homing state, or device features. It therefore introduces no availability restriction beyond an individual form's normal pending state.

### Server action mapping

Extend `controlPrinter` in `frontend/app/actions.ts` to read `axis`, `delta_mm`, and `feedrate_mm_per_min`.

- For `move_axes`, build `movements: [{ axis, delta_mm }]` and include the numeric feedrate.
- For `home`, include `axes: []`.
- For every other existing control, omit both fields and preserve the current request body.
- Never add `required_device_features`.

The Hub remains the public validation boundary. The UI only emits the fixed values declared above and does not duplicate Hub status/model/Homing checks.

### Localization and accessibility

Add English and Chinese `inventory` messages for the trigger, title, description, axis labels, signed movement accessible labels, Home action, confirmation title/message, and confirmation button.

Every movement button has an accessible label containing the axis, signed distance, and millimetre unit. The visible signed values remain compact.

### Web response handling

The existing `controlPrinter` action continues to redirect with `printer_control_queued` on success or the Hub error code on failure. No new client-side error channel is introduced.

## Android Design

### Typed request DTOs

Extend `ControlRequests.kt` with:

- `PrinterAxis`, serialized exactly as `"x"`, `"y"`, or `"z"`;
- `AxisMovementRequest { axis, delta_mm }`;
- `MoveAxesRequest { action: "move_axes", movements, feedrate_mm_per_min }`;
- `HomeRequest { action: "home", axes: [] }`.

Use kotlinx.serialization annotations so default action fields and the empty Home axis list are present in the encoded body. Do not use maps or open-ended JSON values.

### API and repository

Add `home` and `moveAxes` methods to `PandarApi`, both targeting the existing tenant printer-controls route. Add matching thin methods to `PandarRepository` that return the existing `Command` domain type.

### ViewModel and navigation

Add two `PrinterDetailViewModel` methods:

- `home()` sends `HomeRequest()` through the existing `sendControl` lifecycle.
- `moveAxis(axis, deltaMm)` builds a single-movement `MoveAxesRequest`, selecting 3000 mm/min for X/Y and 900 mm/min for Z, then sends it through `sendControl`.

Wire these callbacks through `PandarNavGraph` into `PrinterDetailScreen`. Reuse `PrinterDetailUiState.inFlight`; while any control request is in flight, Android disables the axis and Home controls just as it disables the other printer controls.

### Compose UI

Create `AxisControls.kt` in the printer-detail package and render it as a focused section in `PrinterDetailScreen` near the existing control section.

The section contains:

- the `Move axes` heading;
- X, Y, and Z rows with `-10`, `-1`, `+1`, and `+10` buttons;
- a `Home all axes` button;
- a Material 3 confirmation dialog shown only for Home.

Movement buttons immediately invoke the typed callback. The component does not gate by status, model, device feature, or Homing state. Android retains its current English-only v1 convention; this task does not introduce a new localization architecture.

Every Android movement control exposes a content description containing the axis, signed distance, and millimetre unit. The Home button and confirmation actions likewise have explicit text labels.

### Android response handling

All new actions use the existing `sendControl` path, so queued command ids, command-result events, errors, toasts, and `inFlight` cleanup behave exactly like the current pause, temperature, light, and AMS actions.

## Data Flow

```text
Web form / Android button
  -> fixed axis + signed distance or full Home
  -> existing client control adapter
  -> POST /api/v1/tenants/{tenant}/printers/{printer}/controls
  -> existing Hub semantic operation validation and dispatch
  -> existing Agent printer transport
```

The clients do not interpret `fun`, device-feature bits, MQTT schemas, or printer lifecycle state. Those remain below the semantic control API.

## Error and Safety Behavior

- Home requires explicit confirmation in both clients because it moves all axes.
- Relative moves do not require confirmation.
- The UI does not block movement based on printing status, Homing status, printer model, or device-feature discovery.
- Fixed increments and feedrates are the only values emitted by the UI.
- Hub validation errors flow through each client's existing control error handling with the underlying error code/message preserved.
- Automated verification must not send movement or Homing commands to real printers.

## Rollout and Rollback

The existing Hub and Agent semantic Home/MoveAxes support is a deployment prerequisite. Web and Android contain only client entrypoints and may be released or rolled back independently once that backend support is present.

Rollback removes or reverts the Web and/or Android axis-control entrypoints without changing the Hub operation contract. A client rollback prevents new commands from that client version; it does not cancel commands that the Hub has already queued or that the Agent has already sent. Those commands retain their existing command lifecycle and reporting behavior.

## Test Strategy

### Web

- Add server-action tests proving full Home sends exactly `action` plus an empty `axes` list and no feature requirement.
- Add server-action tests proving X/Y moves produce one nested movement plus feedrate 3000 and Z produces one nested movement plus feedrate 900.
- Preserve existing control request-body tests to catch regressions in unrelated actions.
- Add a component test proving X/Y/Z each expose all four signed increments and Home is available only through its confirmation interaction.
- Run the targeted Vitest tests, frontend lint/type checking, and production Web build.

### Android

- Extend `ControlsBodyShapeTest` with exact serialization assertions for Home and X/Y/Z MoveAxes bodies, including signed values, feedrates, empty Home axes, and absence of `required_device_features`.
- Extract or expose a pure request builder used by `PrinterDetailViewModel.moveAxis`, then test every X/Y/Z, positive/negative, 1/10-mm mapping through that boundary. Assert that X/Y always select 3000, Z always selects 900, and exactly the requested axis and signed delta are serialized.
- Add an instrumentation Compose UI test at `app/src/androidTest/kotlin/.../printerdetail/AxisControlsTest.kt`. Add `androidx.test.ext:junit`, Compose BOM-backed `ui-test-junit4`, and debug-only `ui-test-manifest` dependencies in the version catalog/app build file. The test invokes every signed movement control and proves its callback receives the matching axis and delta. It asserts movement content descriptions include axis, signed distance, and millimetre unit, and asserts Home opens confirmation before invoking its callback.
- Compile the Compose screen and callback wiring through the debug unit-test/build tasks.
- Run Android JVM unit tests, lint, and debug assembly. Boot an Android emulator (the local `Pixel_8_API_36_1` AVD is suitable), wait for `sys.boot_completed=1`, and run `mobile/android/gradlew.bat :app:connectedDebugAndroidTest`. This command is the required execution path for `AxisControlsTest`; `testDebugUnitTest`, lint, and assemble are not substitutes for it.

### Repository-wide

- Run `cargo fmt --all -- --check`.
- Run workspace Clippy with warnings denied.
- Run the full workspace nextest suite.
- Run the repository production-module line-count guard.
- Confirm the existing untracked `crates/pandar-network-plugin/probe-*` directories remain untouched and excluded from the commit.

## Documentation

- Update `docs/android.md` to list XYZ movement and full-axis Home as supported and remove axis jog from the v1 out-of-scope list.
- Update `docs/roadmap.md` with the completed Web/Android axis controls and the next relevant follow-up, without claiming live-printer verification.

## Acceptance Criteria

1. Web users can submit X, Y, or Z moves of `-10`, `-1`, `+1`, or `+10` mm from each printer card.
2. Android users can submit the same movement set from printer detail.
3. Both clients can submit full-axis Home only after a confirmation interaction.
4. X/Y movement bodies contain feedrate 3000; Z movement bodies contain feedrate 900; Home contains an empty axis list.
5. Neither client sends `required_device_features` or adds status/model/Homing/device-feature gates.
6. Existing queued/error feedback behavior is reused without a parallel state path.
7. Web and Android request-shape tests pass, plus their relevant lint/build checks.
8. Rust formatting, Clippy, module-size, and full workspace nextest checks pass.
9. Android and roadmap documentation accurately describe the delivered feature and state that no live printer movement was performed.
10. Android tests deterministically cover every axis/sign/step request mapping and Compose callback, including accessible movement labels.
11. Web and Android can roll back independently without changing or cancelling commands already queued or sent.
