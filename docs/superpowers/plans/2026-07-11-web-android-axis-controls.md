# Web and Android XYZ Axis Controls Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Bambu Studio-style X/Y/Z relative movement and confirmed full-axis Homing to the Web printer cards and Android printer detail screen.

**Architecture:** Both clients submit typed or structured semantic `home` and `move_axes` requests to the existing tenant printer-control endpoint. Web extends its existing server-action adapter and adds a focused dialog component; Android extends its typed Retrofit chain and adds a focused Compose component. Hub, Agent, MQTT, protobuf, database, and network-plugin behavior remain unchanged.

**Tech Stack:** Next.js 16, React 19, next-intl, Vitest, Testing Library, Kotlin 2, Jetpack Compose Material 3, kotlinx.serialization, Retrofit, JUnit 4, AndroidX Compose UI Test.

## Global Constraints

- Follow `docs/superpowers/specs/2026-07-11-web-android-axis-controls-design.md` exactly.
- Expose only `-10`, `-1`, `+1`, and `+10` mm for each X/Y/Z axis.
- X/Y requests use `feedrate_mm_per_min: 3000`; Z requests use `feedrate_mm_per_min: 900`.
- Full Home sends `axes: []` and requires confirmation in both clients.
- Do not send `required_device_features` from Web or Android.
- Do not gate by printer status, model, Homing state, `fun`, or device-feature bits.
- Use typed Kotlin serialization for known request shapes; do not add maps or open-ended JSON.
- Keep production modules at or below 400 lines and do not use Rust `include!`.
- Do not touch or stage any pre-existing `crates/pandar-network-plugin/probe-*` directory.
- Do not send Home or movement commands to physical printers during verification.
- `$sdd-workflow` overrides per-task commits: review each task's diff, but do not commit implementation until final spec-compliance review, docs, and fresh verification are complete.

## File Map

### Web

- `frontend/app/actions.ts`: translate fixed form fields into semantic Home/MoveAxes JSON.
- `frontend/app/actions.test.ts`: exact request-body regression tests.
- `frontend/app/dashboard-printer-axis-controls.tsx`: axis dialog, movement forms, and confirmed Home form.
- `frontend/app/dashboard-printer-axis-controls.test.tsx`: axis form, accessibility, localization, and Home-confirmation tests.
- `frontend/app/dashboard-printer-card.tsx`: render the new focused component after general controls.
- `frontend/app/dashboard-inventory.test.tsx`: prove the axis entrypoint is present on a real printer card.
- `frontend/messages/en.json`, `frontend/messages/zh.json`: localized Web copy.

### Android

- `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/remote/dto/ControlRequests.kt`: typed axis DTOs and pure request builder.
- `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/remote/PandarApi.kt`: Home and MoveAxes Retrofit methods.
- `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/repository/PandarRepository.kt`: thin forwarding methods.
- `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/printerdetail/PrinterDetailViewModel.kt`: existing `sendControl` integration.
- `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/printerdetail/AxisControls.kt`: explicit signed movement buttons and Home confirmation.
- `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/printerdetail/PrinterDetailScreen.kt`: render axis controls.
- `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/navigation/PandarNavGraph.kt`: callback wiring.
- `mobile/android/app/src/test/kotlin/zip/iptables/pandar/android/data/remote/ControlsBodyShapeTest.kt`: exact serialized bodies and all request-builder mappings.
- `mobile/android/app/src/androidTest/kotlin/zip/iptables/pandar/android/ui/printerdetail/AxisControlsTest.kt`: Compose callback, semantics, disabled-state, and Home confirmation tests.
- `mobile/android/gradle/libs.versions.toml`, `mobile/android/app/build.gradle.kts`: Android instrumentation-test dependencies.

### Documentation

- `docs/android.md`: add axis control support and verification commands.
- `docs/roadmap.md`: record Web/Android delivery without live-printer evidence.

## Deferred-Commit Task Review Protocol

Apply this protocol to every implementation task:

1. Dispatch a fresh implementer subagent with only that task's spec/plan excerpts, owned files, constraints, and commands. The implementer must perform the listed RED/GREEN cycle and must not commit.
2. Inspect the implementation and fresh test output locally. For every newly created path in the task, run scoped `git add -N -- <new paths>` so the working-tree diff includes its complete content without staging it for a real commit.
3. Dispatch a fresh spec-compliance reviewer and then a fresh code-quality reviewer. Each reviewer receives the task requirements, complete scoped diff including intent-to-add files, and fresh verification output. Neither reviewer may implement changes.
4. Require the exact task verdict format below. If either reviewer returns `REVISE`, send only the blocking findings to the implementer (or a fresh fix implementer), rerun the affected verification, and repeat both reviews until both return `APPROVE`.
5. Leave all implementation uncommitted until Task 5's final repository-wide review, docs, and verification gates.

```text
VERDICT: APPROVE | REVISE
ISSUES:
- [blocking issue or None]
REQUIRED_CHANGES:
- [change or None]
```

---

### Task 1: Web semantic request adapter

**Files:**

- Modify: `frontend/app/actions.test.ts`
- Modify: `frontend/app/actions.ts`

**Interfaces:**

- Consumes: form fields `action`, `axis`, `delta_mm`, and `feedrate_mm_per_min`.
- Produces: exact Home `{ action, axes: [] }` and MoveAxes `{ action, movements, feedrate_mm_per_min }` request bodies.

- [ ] **Step 1: Add failing exact-body tests**

Add a dedicated `describe` block named `controlPrinter axis operations` beside the existing control tests. Give it its own complete fetch setup so it does not depend on another `describe` block:

```ts
describe("controlPrinter axis operations", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ id: "command-1" }), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
      ),
    );
  });

  // Add the cases below inside this block.
});
```

The file already imports `beforeEach`, `describe`, `expect`, `it`, `vi`, and `controlPrinter`; do not add a second import. Add these cases inside the block:

```ts
it.each([
  ["x", "-10", 3000],
  ["y", "1", 3000],
  ["z", "10", 900],
] as const)(
  "posts a single %s movement with the Studio feedrate",
  async (axis, deltaMm, feedrateMmPerMin) => {
    const formData = new FormData();
    formData.set("tenant_id", "tenant-1");
    formData.set("printer_id", "printer-1");
    formData.set("action", "move_axes");
    formData.set("axis", axis);
    formData.set("delta_mm", deltaMm);
    formData.set("feedrate_mm_per_min", String(feedrateMmPerMin));

    await expect(controlPrinter(formData)).rejects.toThrow(
      "NEXT_REDIRECT:/devices?tenant=tenant-1&status=printer_control_queued",
    );

    const init = vi.mocked(fetch).mock.calls[0][1] as RequestInit;
    expect(JSON.parse(String(init.body))).toEqual({
      action: "move_axes",
      movements: [{ axis, delta_mm: Number(deltaMm) }],
      feedrate_mm_per_min: feedrateMmPerMin,
    });
  },
);

it("posts full-axis Home with an explicit empty axis list", async () => {
  const formData = new FormData();
  formData.set("tenant_id", "tenant-1");
  formData.set("printer_id", "printer-1");
  formData.set("action", "home");

  await expect(controlPrinter(formData)).rejects.toThrow(
    "NEXT_REDIRECT:/devices?tenant=tenant-1&status=printer_control_queued",
  );

  const init = vi.mocked(fetch).mock.calls[0][1] as RequestInit;
  expect(JSON.parse(String(init.body))).toEqual({
    action: "home",
    axes: [],
  });
});
```

- [ ] **Step 2: Run the tests and capture the RED state**

Run from the repository root:

```powershell
npm run test --workspace pandar-web -- app/actions.test.ts
```

Expected: the new MoveAxes cases fail because `movements` and `feedrate_mm_per_min` are absent, and Home fails because `axes` is absent.

- [ ] **Step 3: Implement the minimal request mapping**

In `controlPrinter`, read the three new nullable fields beside the existing fields:

```ts
const axis = nullableField(formData, "axis");
const deltaMm = nullableField(formData, "delta_mm");
const feedrateMmPerMin = nullableField(formData, "feedrate_mm_per_min");
```

Add these properties to the existing `postJson` body without changing existing fields:

```ts
axes: action === "home" ? [] : undefined,
movements:
  action === "move_axes"
    ? [{ axis: axis ?? "", delta_mm: Number(deltaMm) }]
    : undefined,
feedrate_mm_per_min:
  action === "move_axes" && feedrateMmPerMin
    ? Number(feedrateMmPerMin)
    : undefined,
```

The Hub remains the validation boundary for malformed external form submissions. `JSON.stringify` omits every `undefined` property, so existing action bodies stay unchanged.

- [ ] **Step 4: Run focused and existing Web tests GREEN**

```powershell
npm run test --workspace pandar-web -- app/actions.test.ts
```

Expected: all `actions.test.ts` tests pass, including exact equality that proves `required_device_features` is absent.

- [ ] **Step 5: Inspect the task diff before review**

```powershell
git diff --check -- frontend/app/actions.ts frontend/app/actions.test.ts
git diff -- frontend/app/actions.ts frontend/app/actions.test.ts
```

Expected: only the new form-to-semantic mapping and its tests changed. Apply the deferred-commit task review protocol; do not commit.

---

### Task 2: Web axis dialog and confirmed Home

**Files:**

- Create: `frontend/app/dashboard-printer-axis-controls.tsx`
- Create: `frontend/app/dashboard-printer-axis-controls.test.tsx`
- Modify: `frontend/app/dashboard-printer-card.tsx`
- Modify: `frontend/app/dashboard-inventory.test.tsx`
- Modify: `frontend/messages/en.json`
- Modify: `frontend/messages/zh.json`

**Interfaces:**

- Consumes: `Printer { tenant_id, id }` and the `controlPrinter` server action from Task 1.
- Produces: `PrinterAxisControls({ printer }: { printer: Printer })`.

- [ ] **Step 1: Add failing component and integration tests**

Create `dashboard-printer-axis-controls.test.tsx` with this complete scaffold:

```tsx
import { NextIntlClientProvider } from "next-intl";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import zh from "../messages/zh.json";
import { PrinterAxisControls } from "./dashboard-printer-axis-controls";
import type { Printer } from "./dashboard-types";

const controlPrinterMock = vi.hoisted(() =>
  vi.fn(async (_formData: FormData) => undefined),
);

vi.mock("./actions", () => ({
  controlPrinter: controlPrinterMock,
}));

function renderWithMessages(children: React.ReactNode, locale = "en") {
  return render(
    <NextIntlClientProvider
      locale={locale}
      messages={locale === "zh" ? zh : en}
    >
      {children}
    </NextIntlClientProvider>,
  );
}

const printer: Printer = {
  id: "printer-1",
  tenant_id: "tenant-1",
  agent_id: "agent-1",
  serial_number: "SERIAL123",
  name: "Office A1",
  model: "A1",
  status: "idle",
  last_seen_at: "2026-07-02T00:00:00Z",
  created_at: "2026-07-02T00:00:00Z",
  materials: null,
};

describe("PrinterAxisControls", () => {
  beforeEach(() => {
    controlPrinterMock.mockClear();
  });

  // Add the cases below inside this block.
});
```

Add these cases inside the `describe` block:

```tsx
const axes = ["X", "Y", "Z"] as const;
const distances = [-10, -1, 1, 10] as const;

it("renders exact signed axis forms without status or feature gates", async () => {
  const user = userEvent.setup();
  const offlinePrinter = Object.assign({}, printer, { status: "offline" });
  renderWithMessages(<PrinterAxisControls printer={offlinePrinter} />);

  const trigger = screen.getByRole("button", { name: "Move axes" });
  expect(trigger).toBeEnabled();
  await user.click(trigger);

  for (const axis of axes) {
    for (const distance of distances) {
      const signed = distance > 0 ? `+${distance}` : String(distance);
      const button = screen.getByRole("button", {
        name: `Move ${axis} by ${signed} mm`,
      });
      expect(button).toBeEnabled();
      const form = button.closest("form");
      expect(form?.querySelector('input[name="action"]')).toHaveValue(
        "move_axes",
      );
      expect(form?.querySelector('input[name="axis"]')).toHaveValue(
        axis.toLowerCase(),
      );
      expect(form?.querySelector('input[name="delta_mm"]')).toHaveValue(
        String(distance),
      );
      expect(
        form?.querySelector('input[name="feedrate_mm_per_min"]'),
      ).toHaveValue(axis === "Z" ? "900" : "3000");
      expect(
        form?.querySelector('input[name="required_device_features"]'),
      ).toBeNull();
    }
  }
});

it("requires confirmation before full-axis Home", async () => {
  const user = userEvent.setup();
  renderWithMessages(<PrinterAxisControls printer={printer} />);
  await user.click(screen.getByRole("button", { name: "Move axes" }));
  await user.click(screen.getByRole("button", { name: "Home all axes" }));

  expect(controlPrinterMock).not.toHaveBeenCalled();
  expect(screen.getByRole("dialog", { name: "Auto homing" })).toBeVisible();
  const homeForm = screen
    .getByRole("button", { name: "Home all axes" })
    .closest("form");
  expect(homeForm?.querySelector('input[name="action"]')).toHaveValue("home");
  expect(
    homeForm?.querySelector('input[name="required_device_features"]'),
  ).toBeNull();
  await user.click(screen.getByRole("button", { name: "Homing" }));
  await waitFor(() => expect(controlPrinterMock).toHaveBeenCalledTimes(1));
  const submitted = controlPrinterMock.mock.calls[0][0];
  expect(Object.fromEntries(submitted.entries())).toEqual({
    tenant_id: "tenant-1",
    printer_id: "printer-1",
    action: "home",
  });
});

it("renders localized Chinese axis controls", async () => {
  const user = userEvent.setup();
  renderWithMessages(<PrinterAxisControls printer={printer} />, "zh");
  await user.click(screen.getByRole("button", { name: "移动轴" }));
  expect(screen.getByRole("heading", { name: "移动打印机轴" })).toBeVisible();
  expect(
    screen.getByRole("button", { name: "将 X 轴移动 +10 毫米" }),
  ).toBeVisible();
});
```

In `dashboard-inventory.test.tsx`, add one assertion to the existing printer-controls test:

```ts
expect(within(card).getByRole("button", { name: "Move axes" })).toBeVisible();
```

- [ ] **Step 2: Run the component tests and capture the RED state**

```powershell
npm run test --workspace pandar-web -- app/dashboard-printer-axis-controls.test.tsx app/dashboard-inventory.test.tsx
```

Expected: compilation fails because `PrinterAxisControls` and its translation keys do not exist.

- [ ] **Step 3: Add exact localized messages**

Add these keys under `inventory` in `en.json`:

```json
"moveAxes": "Move axes",
"moveAxesTitle": "Move printer axes",
"moveAxesDescription": "Move one axis by a fixed distance, or home all axes.",
"closeMoveAxes": "Close axis controls",
"axisLabel": "{axis} axis",
"moveAxisBy": "Move {axis} by {distance} mm",
"homeAxes": "Home all axes",
"homeAxesTitle": "Auto homing",
"homeAxesMessage": "Are you sure you want to trigger auto homing?",
"homeAxesConfirm": "Homing"
```

Add the matching keys under `inventory` in `zh.json`:

```json
"moveAxes": "移动轴",
"moveAxesTitle": "移动打印机轴",
"moveAxesDescription": "按固定距离移动单个轴，或将全部轴归位。",
"closeMoveAxes": "关闭轴控制",
"axisLabel": "{axis} 轴",
"moveAxisBy": "将 {axis} 轴移动 {distance} 毫米",
"homeAxes": "全部轴归位",
"homeAxesTitle": "自动归位",
"homeAxesMessage": "确定要触发自动归位吗？",
"homeAxesConfirm": "归位"
```

- [ ] **Step 4: Implement the focused Web component**

Create `dashboard-printer-axis-controls.tsx` with:

```tsx
"use client";

import { Axis3dIcon } from "lucide-react";
import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";

import { controlPrinter } from "./actions";
import { ConfirmForm } from "./confirm-dialog";
import type { Printer } from "./dashboard-types";

const AXES = [
  { id: "x", label: "X", feedrate: 3000 },
  { id: "y", label: "Y", feedrate: 3000 },
  { id: "z", label: "Z", feedrate: 900 },
] as const;
const DISTANCES_MM = [-10, -1, 1, 10] as const;

export function PrinterAxisControls({ printer }: { printer: Printer }) {
  const t = useTranslations("inventory");

  return (
    <div className="mt-2">
      <Dialog>
        <DialogTrigger
          className="inline-flex h-8 w-full items-center justify-center gap-1.5 rounded-md bg-primary/10 px-2 text-sm font-semibold text-primary transition hover:bg-primary/15"
          type="button"
        >
          <Axis3dIcon className="size-4" />
          {t("moveAxes")}
        </DialogTrigger>
        <DialogContent closeLabel={t("closeMoveAxes")}>
          <DialogHeader>
            <DialogTitle>{t("moveAxesTitle")}</DialogTitle>
            <DialogDescription>{t("moveAxesDescription")}</DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            {AXES.map((axis) => (
              <div
                className="grid grid-cols-[3rem_1fr] items-center gap-2"
                key={axis.id}
              >
                <span className="text-sm font-semibold">
                  {t("axisLabel", { axis: axis.label })}
                </span>
                <div className="grid grid-cols-4 gap-1.5">
                  {DISTANCES_MM.map((distance) => {
                    const signed =
                      distance > 0 ? `+${distance}` : String(distance);
                    return (
                      <form action={controlPrinter} key={distance}>
                        <input
                          name="tenant_id"
                          type="hidden"
                          value={printer.tenant_id}
                        />
                        <input
                          name="printer_id"
                          type="hidden"
                          value={printer.id}
                        />
                        <input name="action" type="hidden" value="move_axes" />
                        <input name="axis" type="hidden" value={axis.id} />
                        <input name="delta_mm" type="hidden" value={distance} />
                        <input
                          name="feedrate_mm_per_min"
                          type="hidden"
                          value={axis.feedrate}
                        />
                        <Button
                          aria-label={t("moveAxisBy", {
                            axis: axis.label,
                            distance: signed,
                          })}
                          className="w-full"
                          size="sm"
                          type="submit"
                          variant="outline"
                        >
                          {signed}
                        </Button>
                      </form>
                    );
                  })}
                </div>
              </div>
            ))}
            <ConfirmForm
              action={controlPrinter}
              buttonClassName="inline-flex h-8 w-full items-center justify-center rounded-md bg-muted px-2 text-sm font-semibold text-foreground hover:bg-muted/80"
              buttonLabel={t("homeAxes")}
              confirmLabel={t("homeAxesConfirm")}
              message={t("homeAxesMessage")}
              title={t("homeAxesTitle")}
              tone="default"
            >
              <input name="tenant_id" type="hidden" value={printer.tenant_id} />
              <input name="printer_id" type="hidden" value={printer.id} />
              <input name="action" type="hidden" value="home" />
            </ConfirmForm>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
```

- [ ] **Step 5: Render the component from the printer card**

Import `PrinterAxisControls` in `dashboard-printer-card.tsx` and add this immediately after `<PrinterControlsPanel printer={printer} />`:

```tsx
<PrinterAxisControls printer={printer} />
```

Do not add status, model, or feature predicates.

- [ ] **Step 6: Run focused tests and Web verification GREEN**

```powershell
npm run test --workspace pandar-web -- app/dashboard-printer-axis-controls.test.tsx app/dashboard-inventory.test.tsx app/actions.test.ts
npm run test:web
npm run build:web
```

Expected: all Web tests pass and the production build completes. The current `next lint` script is not a valid Next 16 command, so the production build and its TypeScript compilation are the applicable Web static check; do not modify unrelated lint tooling.

- [ ] **Step 7: Inspect the task diff before review**

```powershell
git add -N -- frontend/app/dashboard-printer-axis-controls.tsx frontend/app/dashboard-printer-axis-controls.test.tsx
git diff --check -- frontend/app frontend/messages
git diff -- frontend/app/dashboard-printer-axis-controls.tsx frontend/app/dashboard-printer-axis-controls.test.tsx frontend/app/dashboard-printer-card.tsx frontend/app/dashboard-inventory.test.tsx frontend/messages/en.json frontend/messages/zh.json
```

Expected: the complete new component and test are visible in the diff together with only the Web integration/translations. Apply the deferred-commit task review protocol; do not commit.

---

### Task 3: Android typed request and control chain

**Files:**

- Modify: `mobile/android/app/src/test/kotlin/zip/iptables/pandar/android/data/remote/ControlsBodyShapeTest.kt`
- Modify: `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/remote/dto/ControlRequests.kt`
- Modify: `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/remote/PandarApi.kt`
- Modify: `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/repository/PandarRepository.kt`
- Modify: `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/printerdetail/PrinterDetailViewModel.kt`

**Interfaces:**

- Produces: `PrinterAxis`, `HomeRequest`, `AxisMovementRequest`, `MoveAxesRequest`, and `moveAxisRequest(axis, deltaMm)`.
- Produces: repository/ViewModel `home()` and `moveAxis(axis: PrinterAxis, deltaMm: Double)` methods used by Task 4.

- [ ] **Step 1: Add failing typed-body and mapping tests**

Add these exact imports beside the existing DTO imports in `ControlsBodyShapeTest.kt`:

```kotlin
import zip.iptables.pandar.android.data.remote.dto.HomeRequest
import zip.iptables.pandar.android.data.remote.dto.PrinterAxis
import zip.iptables.pandar.android.data.remote.dto.moveAxisRequest
```

The file already imports `kotlinx.serialization.encodeToString`, `assertEquals`, `assertFalse`, and `Test`. Add these cases to the existing class:

```kotlin
@Test fun home_all_axes_is_explicit() =
    assertEquals(
        """{"action":"home","axes":[]}""",
        json.encodeToString(HomeRequest()),
    )

@Test fun every_axis_sign_and_step_maps_to_the_exact_request() {
    val cases = listOf(
        Triple(PrinterAxis.X, -10.0, 3000),
        Triple(PrinterAxis.X, -1.0, 3000),
        Triple(PrinterAxis.X, 1.0, 3000),
        Triple(PrinterAxis.X, 10.0, 3000),
        Triple(PrinterAxis.Y, -10.0, 3000),
        Triple(PrinterAxis.Y, -1.0, 3000),
        Triple(PrinterAxis.Y, 1.0, 3000),
        Triple(PrinterAxis.Y, 10.0, 3000),
        Triple(PrinterAxis.Z, -10.0, 900),
        Triple(PrinterAxis.Z, -1.0, 900),
        Triple(PrinterAxis.Z, 1.0, 900),
        Triple(PrinterAxis.Z, 10.0, 900),
    )

    cases.forEach { (axis, deltaMm, feedrate) ->
        val encoded = json.encodeToString(moveAxisRequest(axis, deltaMm))
        val expectedAxis = axis.name.lowercase()
        assertEquals(
            """{"action":"move_axes","movements":[{"axis":"$expectedAxis","delta_mm":$deltaMm}],"feedrate_mm_per_min":$feedrate}""",
            encoded,
        )
        assertFalse(encoded.contains("required_device_features"))
    }
}
```

- [ ] **Step 2: Run the JVM test and capture the RED state**

```powershell
Set-Location mobile/android
.\gradlew.bat :app:testDebugUnitTest --tests "zip.iptables.pandar.android.data.remote.ControlsBodyShapeTest"
Set-Location ../..
```

Expected: Kotlin compilation fails because the new request types and builder do not exist.

- [ ] **Step 3: Add typed request DTOs and exhaustive builder**

Append to `ControlRequests.kt`:

```kotlin
@Serializable
enum class PrinterAxis {
    @SerialName("x") X,
    @SerialName("y") Y,
    @SerialName("z") Z,
}

@Serializable
data class HomeRequest(
    @SerialName("action") @EncodeDefault val action: String = "home",
    @SerialName("axes") @EncodeDefault val axes: List<PrinterAxis> = emptyList(),
)

@Serializable
data class AxisMovementRequest(
    @SerialName("axis") val axis: PrinterAxis,
    @SerialName("delta_mm") val deltaMm: Double,
)

@Serializable
data class MoveAxesRequest(
    @SerialName("action") @EncodeDefault val action: String = "move_axes",
    @SerialName("movements") val movements: List<AxisMovementRequest>,
    @SerialName("feedrate_mm_per_min") val feedrateMmPerMin: Int,
)

fun moveAxisRequest(axis: PrinterAxis, deltaMm: Double) = MoveAxesRequest(
    movements = listOf(AxisMovementRequest(axis = axis, deltaMm = deltaMm)),
    feedrateMmPerMin = when (axis) {
        PrinterAxis.X, PrinterAxis.Y -> 3000
        PrinterAxis.Z -> 900
    },
)
```

The exhaustive `when` has no `else`, so adding a future axis requires an explicit feedrate decision.

- [ ] **Step 4: Add Retrofit and repository methods**

Add these imports to `PandarApi.kt`:

```kotlin
import zip.iptables.pandar.android.data.remote.dto.HomeRequest
import zip.iptables.pandar.android.data.remote.dto.MoveAxesRequest
```

Then add to `PandarApi`:

```kotlin
@POST("api/v1/tenants/{tenant}/printers/{printer}/controls")
suspend fun home(
    @Path("tenant") tenant: String,
    @Path("printer") printer: String,
    @Body body: HomeRequest = HomeRequest(),
): CommandResponseDto

@POST("api/v1/tenants/{tenant}/printers/{printer}/controls")
suspend fun moveAxes(
    @Path("tenant") tenant: String,
    @Path("printer") printer: String,
    @Body body: MoveAxesRequest,
): CommandResponseDto
```

Add these imports to `PandarRepository.kt`:

```kotlin
import zip.iptables.pandar.android.data.remote.dto.MoveAxesRequest
```

Then add beside other printer controls in `PandarRepository`:

```kotlin
suspend fun home(printerId: String): Command =
    api.home(tenant(), printerId).toDomain()

suspend fun moveAxes(printerId: String, body: MoveAxesRequest): Command =
    api.moveAxes(tenant(), printerId, body).toDomain()
```

- [ ] **Step 5: Reuse the ViewModel command lifecycle**

Add these imports to `PrinterDetailViewModel.kt`:

```kotlin
import zip.iptables.pandar.android.data.remote.dto.PrinterAxis
import zip.iptables.pandar.android.data.remote.dto.moveAxisRequest
```

Then add beside the existing control methods:

```kotlin
fun home() = sendControl { container.pandar.home(printerId) }

fun moveAxis(axis: PrinterAxis, deltaMm: Double) = sendControl {
    container.pandar.moveAxes(printerId, moveAxisRequest(axis, deltaMm))
}
```

Do not add a parallel loading/error/toast state.

- [ ] **Step 6: Run Android JVM tests GREEN**

```powershell
Set-Location mobile/android
.\gradlew.bat :app:testDebugUnitTest
Set-Location ../..
```

Expected: all Android JVM tests pass, including all 12 movement mappings and exact Home JSON.

- [ ] **Step 7: Inspect the task diff before review**

```powershell
git diff --check -- mobile/android/app/src/main mobile/android/app/src/test
git diff --stat -- mobile/android/app/src/main mobile/android/app/src/test
```

Expected: only typed Android request/control-chain code and unit tests changed. Apply the deferred-commit task review protocol; do not commit.

---

### Task 4: Android Compose axis controls and instrumentation coverage

**Files:**

- Modify: `mobile/android/gradle/libs.versions.toml`
- Modify: `mobile/android/app/build.gradle.kts`
- Create: `mobile/android/app/src/androidTest/kotlin/zip/iptables/pandar/android/ui/printerdetail/AxisControlsTest.kt`
- Create: `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/printerdetail/AxisControls.kt`
- Modify: `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/printerdetail/PrinterDetailScreen.kt`
- Modify: `mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/navigation/PandarNavGraph.kt`

**Interfaces:**

- Consumes: `PrinterAxis` plus ViewModel methods from Task 3.
- Produces: `AxisControls(enabled, onHome, onMoveAxis)` and printer-detail callback wiring.

- [ ] **Step 1: Add Android instrumentation dependencies**

Add versions and aliases to `libs.versions.toml`:

```toml
androidxTestExtJunit = "1.2.1"
androidxTestRunner = "1.6.2"

androidx-test-ext-junit = { group = "androidx.test.ext", name = "junit", version.ref = "androidxTestExtJunit" }
androidx-test-runner = { group = "androidx.test", name = "runner", version.ref = "androidxTestRunner" }
compose-ui-test-junit4 = { group = "androidx.compose.ui", name = "ui-test-junit4" }
compose-ui-test-manifest = { group = "androidx.compose.ui", name = "ui-test-manifest" }
```

Add dependencies to `app/build.gradle.kts`:

```kotlin
androidTestImplementation(platform(libs.compose.bom))
androidTestImplementation(libs.androidx.test.ext.junit)
androidTestImplementation(libs.androidx.test.runner)
androidTestImplementation(libs.compose.ui.test.junit4)
debugImplementation(libs.compose.ui.test.manifest)
```

- [ ] **Step 2: Add failing Compose UI tests**

Create `AxisControlsTest.kt` with this exact package/import scaffold followed by the three cases:

```kotlin
package zip.iptables.pandar.android.ui.printerdetail

import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import zip.iptables.pandar.android.data.remote.dto.PrinterAxis
```

Then add:

```kotlin
@RunWith(AndroidJUnit4::class)
class AxisControlsTest {
    @get:Rule val composeRule = createComposeRule()

    @Test fun every_signed_button_dispatches_its_exact_axis_and_distance() {
        val calls = mutableListOf<Pair<PrinterAxis, Double>>()
        composeRule.setContent {
            MaterialTheme {
                AxisControls(
                    enabled = true,
                    onHome = {},
                    onMoveAxis = { axis, deltaMm -> calls += axis to deltaMm },
                )
            }
        }
        val cases = PrinterAxis.entries.flatMap { axis ->
            listOf(-10.0, -1.0, 1.0, 10.0).map { deltaMm -> axis to deltaMm }
        }
        cases.forEach { (axis, deltaMm) ->
            val signed = if (deltaMm > 0) "+${deltaMm.toInt()}" else deltaMm.toInt().toString()
            composeRule
                .onNodeWithContentDescription("Move ${axis.name} by $signed mm")
                .performClick()
        }
        composeRule.runOnIdle { assertEquals(cases, calls) }
    }

    @Test fun home_requires_confirmation() {
        var homeCalls = 0
        composeRule.setContent {
            MaterialTheme {
                AxisControls(
                    enabled = true,
                    onHome = { homeCalls += 1 },
                    onMoveAxis = { _, _ -> },
                )
            }
        }
        composeRule.onNodeWithText("Home all axes").performClick()
        composeRule.runOnIdle { assertEquals(0, homeCalls) }
        composeRule.onNodeWithText("Are you sure you want to trigger auto homing?").assertIsDisplayed()
        composeRule.onNodeWithText("Homing").performClick()
        composeRule.runOnIdle { assertEquals(1, homeCalls) }
    }

    @Test fun in_flight_disables_all_printer_command_buttons() {
        composeRule.setContent {
            MaterialTheme {
                AxisControls(enabled = false, onHome = {}, onMoveAxis = { _, _ -> })
            }
        }
        PrinterAxis.entries.forEach { axis ->
            listOf(-10, -1, 1, 10).forEach { deltaMm ->
                val signed = if (deltaMm > 0) "+$deltaMm" else deltaMm.toString()
                composeRule
                    .onNodeWithContentDescription("Move ${axis.name} by $signed mm")
                    .assertIsNotEnabled()
            }
        }
        composeRule.onNodeWithText("Home all axes").assertIsNotEnabled()
    }
}
```

- [ ] **Step 3: Compile the instrumentation test and capture RED**

```powershell
Set-Location mobile/android
.\gradlew.bat :app:compileDebugAndroidTestKotlin
Set-Location ../..
```

Expected: compilation fails because `AxisControls` does not exist.

- [ ] **Step 4: Implement explicit signed Compose controls**

Create `AxisControls.kt` with this exact package/import scaffold:

```kotlin
package zip.iptables.pandar.android.ui.printerdetail

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import zip.iptables.pandar.android.data.remote.dto.PrinterAxis
```

Then add this task interface and behavior:

```kotlin
@Composable
internal fun AxisControls(
    enabled: Boolean,
    onHome: () -> Unit,
    onMoveAxis: (PrinterAxis, Double) -> Unit,
) {
    var confirmHome by remember { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("Move axes", style = MaterialTheme.typography.titleMedium)
        PrinterAxis.entries.forEach { axis ->
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(axis.name, modifier = Modifier.width(20.dp), fontWeight = FontWeight.SemiBold)
                listOf(-10.0, -1.0, 1.0, 10.0).forEach { deltaMm ->
                    val signed = if (deltaMm > 0) "+${deltaMm.toInt()}" else deltaMm.toInt().toString()
                    OutlinedButton(
                        onClick = { onMoveAxis(axis, deltaMm) },
                        enabled = enabled,
                        modifier = Modifier
                            .weight(1f)
                            .semantics {
                                contentDescription = "Move ${axis.name} by $signed mm"
                            },
                    ) {
                        Text(signed)
                    }
                }
            }
        }
        OutlinedButton(
            onClick = { confirmHome = true },
            enabled = enabled,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Home all axes")
        }
    }
    if (confirmHome) {
        AlertDialog(
            onDismissRequest = { confirmHome = false },
            title = { Text("Auto homing") },
            text = { Text("Are you sure you want to trigger auto homing?") },
            confirmButton = {
                TextButton(onClick = {
                    confirmHome = false
                    onHome()
                }) { Text("Homing") }
            },
            dismissButton = {
                TextButton(onClick = { confirmHome = false }) { Text("Cancel") }
            },
        )
    }
}
```

Do not add printer-state or feature inputs.

- [ ] **Step 5: Wire the screen and navigation**

Add to `PrinterDetailScreen` parameters:

```kotlin
onHome: () -> Unit,
onMoveAxis: (PrinterAxis, Double) -> Unit,
```

Add this import to `PrinterDetailScreen.kt`:

```kotlin
import zip.iptables.pandar.android.data.remote.dto.PrinterAxis
```

After `PrintActionsRow` and its divider, render:

```kotlin
AxisControls(
    enabled = !state.inFlight,
    onHome = onHome,
    onMoveAxis = onMoveAxis,
)
HorizontalDivider()
```

Add to the `PandarNavGraph` printer-detail call:

```kotlin
onHome = { vm.home() },
onMoveAxis = { axis, deltaMm -> vm.moveAxis(axis, deltaMm) },
```

- [ ] **Step 6: Run local Android compile/JVM checks GREEN**

```powershell
Set-Location mobile/android
.\gradlew.bat :app:testDebugUnitTest :app:assembleDebug :app:lintDebug
Set-Location ../..
```

Expected: JVM tests, APK assembly, and Android lint all complete successfully.

- [ ] **Step 7: Boot the hidden emulator and run Compose instrumentation GREEN**

From PowerShell, start the existing AVD without a visible window:

```powershell
$emulator = "$env:LOCALAPPDATA\Android\Sdk\emulator\emulator.exe"
$adb = "$env:LOCALAPPDATA\Android\Sdk\platform-tools\adb.exe"
Start-Process -FilePath $emulator -ArgumentList @('-avd', 'Pixel_8_API_36_1', '-no-window', '-no-audio', '-no-boot-anim') -WindowStyle Hidden
& $adb wait-for-device
do {
  Start-Sleep -Seconds 2
  $booted = (& $adb shell getprop sys.boot_completed).Trim()
} until ($booted -eq '1')
Set-Location mobile/android
.\gradlew.bat :app:connectedDebugAndroidTest
Set-Location ../..
```

Expected: all three `AxisControlsTest` instrumentation tests pass. These tests only exercise local callbacks and never call the Hub or a printer.

- [ ] **Step 8: Inspect the task diff before review**

```powershell
git add -N -- mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/printerdetail/AxisControls.kt mobile/android/app/src/androidTest/kotlin/zip/iptables/pandar/android/ui/printerdetail/AxisControlsTest.kt
git diff --check -- mobile/android
git diff -- mobile/android/gradle/libs.versions.toml mobile/android/app/build.gradle.kts mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/printerdetail/AxisControls.kt mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/printerdetail/PrinterDetailScreen.kt mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/navigation/PandarNavGraph.kt mobile/android/app/src/androidTest/kotlin/zip/iptables/pandar/android/ui/printerdetail/AxisControlsTest.kt
```

Expected: the complete new Compose component and instrumentation test are visible together with only Android UI wiring and test dependencies. Apply the deferred-commit task review protocol; do not commit.

---

### Task 5: Final spec review, documentation, verification, commit, and push

**Files:**

- Modify after final implementation approval: `docs/android.md`
- Modify after final implementation approval: `docs/roadmap.md`

**Interfaces:**

- Consumes: reviewed Web and Android implementation from Tasks 1-4.
- Produces: current documentation, fresh verification evidence, one final implementation commit, and a pushed branch.

- [ ] **Step 1: Run the required final implementation review gate before docs**

Refresh intent-to-add entries and produce a complete implementation diff that includes every new file:

```powershell
git add -N -- docs/superpowers/plans/2026-07-11-web-android-axis-controls.md frontend/app/dashboard-printer-axis-controls.tsx frontend/app/dashboard-printer-axis-controls.test.tsx mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/printerdetail/AxisControls.kt mobile/android/app/src/androidTest/kotlin/zip/iptables/pandar/android/ui/printerdetail/AxisControlsTest.kt
git diff --check HEAD -- frontend mobile/android
git diff HEAD -- frontend mobile/android
```

Provide the reviewed spec, reviewed plan, base/head SHAs, the complete diff above, and current task verification output to a fresh independent subagent and default-model OpenCode. Both must return the exact `VERDICT: APPROVE` spec-implementation format. If either revises, dispatch a fresh bounded fix implementer, rerun affected tests, refresh the complete diff, and rerun both reviewers.

- [ ] **Step 2: Update Android documentation after the review gate**

In `docs/android.md`:

- Add XYZ `-10/-1/+1/+10` movement and confirmed full-axis Home to the per-printer detail feature list.
- Remove `axis jog` from the v1 out-of-scope sentence.
- Add `./gradlew :app:connectedDebugAndroidTest` to Build and test.
- State that UI/instrumentation verification does not send real printer movement commands.

- [ ] **Step 3: Update the roadmap after the review gate**

At the top of `## Completed`, add one bullet recording Web and Android X/Y/Z movement, fixed Studio feedrates, confirmed full-axis Home, exact request-shape tests, and Android instrumentation coverage. State that no live printer was moved or homed.

Under `## Completed: Android App`, update the printer-detail bullet to include XYZ movement and full-axis Home. Keep the existing immediate-next real hardware probe item unchanged.

- [ ] **Step 4: Run fresh Web and Android verification**

```powershell
npm run test:web
npm run build:web
Set-Location mobile/android
.\gradlew.bat :app:testDebugUnitTest :app:assembleDebug :app:lintDebug
.\gradlew.bat :app:connectedDebugAndroidTest
Set-Location ../..
```

Expected: all Web tests/build and Android JVM/build/lint/instrumentation checks pass.

- [ ] **Step 5: Run mandatory fresh Rust workspace verification**

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path Cargo.toml --workspace
cargo nextest run -p pandar-core --test module_size
```

Expected: formatting and Clippy complete with no errors; the full workspace and module-size tests report zero failures. No database code changed. If `PANDAR_TEST_POSTGRES_URL` is unset, record the existing real-PostgreSQL coverage skip exactly rather than claiming it ran.

- [ ] **Step 6: Audit the final diff and protected paths**

```powershell
git add -N -- docs/superpowers/plans/2026-07-11-web-android-axis-controls.md frontend/app/dashboard-printer-axis-controls.tsx frontend/app/dashboard-printer-axis-controls.test.tsx mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/printerdetail/AxisControls.kt mobile/android/app/src/androidTest/kotlin/zip/iptables/pandar/android/ui/printerdetail/AxisControlsTest.kt
git status --short
git diff --check HEAD
git diff --stat HEAD
git diff HEAD -- . ':(exclude)crates/pandar-network-plugin/probe-*'
```

Expected: only the reviewed spec commit plus intended Web, Android, plan, and docs changes are present. Every `probe-*` directory remains untracked and unstaged.

- [ ] **Step 7: Commit with Conventional Commits**

Stage only the plan, Web, Android, and documentation files. Do not stage `probe-*`.

```powershell
git add -- docs/superpowers/plans/2026-07-11-web-android-axis-controls.md docs/android.md docs/roadmap.md frontend/app/actions.ts frontend/app/actions.test.ts frontend/app/dashboard-printer-axis-controls.tsx frontend/app/dashboard-printer-axis-controls.test.tsx frontend/app/dashboard-printer-card.tsx frontend/app/dashboard-inventory.test.tsx frontend/messages/en.json frontend/messages/zh.json mobile/android/gradle/libs.versions.toml mobile/android/app/build.gradle.kts mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/remote/dto/ControlRequests.kt mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/remote/PandarApi.kt mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/data/repository/PandarRepository.kt mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/printerdetail/PrinterDetailViewModel.kt mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/printerdetail/AxisControls.kt mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/printerdetail/PrinterDetailScreen.kt mobile/android/app/src/main/kotlin/zip/iptables/pandar/android/ui/navigation/PandarNavGraph.kt mobile/android/app/src/test/kotlin/zip/iptables/pandar/android/data/remote/ControlsBodyShapeTest.kt mobile/android/app/src/androidTest/kotlin/zip/iptables/pandar/android/ui/printerdetail/AxisControlsTest.kt
git diff --cached --check
git commit -m "feat(ui): add web and android axis controls"
```

Expected: the implementation commit succeeds and contains no probe directory.

- [ ] **Step 8: Push and verify the remote ref**

```powershell
git push
git rev-parse HEAD
git rev-parse '@{upstream}'
```

Expected: push succeeds and local `HEAD` equals the upstream ref. If credentials, network, remote rejection, or branch policy blocks push, report the local commit SHA and exact push error.
