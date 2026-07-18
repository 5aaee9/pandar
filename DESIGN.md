<!-- SEED: re-run $impeccable document once design tokens live in CSS to capture the actual values, tonal ramps, and component CSS. -->

---

name: Pandar
description: Calm, technical operations console for self-hosted Bambu printer infrastructure.

---

# Design System: Pandar

## 1. Overview

**Creative North Star: "The Instrument Console"**

Pandar is infrastructure software for repeated operational use, not a marketing page. It should feel like a precision instrument panel: dense, scannable, quiet at rest, and loud only when state demands attention. The operator glances at it many times a day to confirm health and act on exceptions, so the interface must disappear into the task. Trust comes from precision and consistency, never from decoration.

The system is **Restrained**. A neutral black/white token set carries structure, hierarchy, and focus without decorative color. Surfaces are white at the page and card level, with near-neutral gray layers for secondary, muted, accent, sidebar, and border treatment. Typography is one well-tuned sans (Inter) at a fixed rem scale, with a monospace reserved for machine identifiers — serial numbers, agent IDs, job codes — because tabular, identifiable data is the substance of this product.

This system explicitly rejects decorative SaaS landing-page layouts, oversized hero content inside the app, playful consumer-device styling, and any visual pattern that obscures operational state. If a choice makes the dashboard feel like a consumer gadget or a marketing site, it is wrong.

**Key Characteristics:**

- Calm and technical; trustworthy by precision, not by ornament.
- Operational state is scannable at a glance; exceptions break the calm deliberately.
- Restrained neutral tokens used for action, selection, and state — never decoration.
- Monospace for machine identifiers; one sans for everything else.
- Explicit tenant and agent boundaries expressed through layout and surface layering.
- Density is a virtue; the same visual vocabulary recurs screen to screen.

## 2. Colors

The palette is a neutral OKLCH system. The app background, cards, and popovers are white in light mode; hierarchy comes from black foreground, soft neutral fills, and hairline borders. Dark mode inverts the same neutral structure.

### Current Token Palette

```css
:root {
  --background: oklch(1 0 0);
  --foreground: oklch(0.145 0 0);
  --card: oklch(1 0 0);
  --card-foreground: oklch(0.145 0 0);
  --popover: oklch(1 0 0);
  --popover-foreground: oklch(0.145 0 0);
  --primary: oklch(0.205 0 0);
  --primary-foreground: oklch(0.985 0 0);
  --secondary: oklch(0.97 0 0);
  --secondary-foreground: oklch(0.205 0 0);
  --muted: oklch(0.97 0 0);
  --muted-foreground: oklch(0.556 0 0);
  --accent: oklch(0.97 0 0);
  --accent-foreground: oklch(0.205 0 0);
  --destructive: oklch(0.577 0.245 27.325);
  --success: oklch(0.508 0.118 165.612);
  --warning: oklch(0.555 0.163 48.998);
  --border: oklch(0.922 0 0);
  --input: oklch(0.922 0 0);
  --ring: oklch(0.708 0 0);
  --chart-1: oklch(0.87 0 0);
  --chart-2: oklch(0.556 0 0);
  --chart-3: oklch(0.439 0 0);
  --chart-4: oklch(0.371 0 0);
  --chart-5: oklch(0.269 0 0);
  --radius: 0.625rem;
  --sidebar: oklch(0.985 0 0);
  --sidebar-foreground: oklch(0.145 0 0);
  --sidebar-primary: oklch(0.205 0 0);
  --sidebar-primary-foreground: oklch(0.985 0 0);
  --sidebar-accent: oklch(0.97 0 0);
  --sidebar-accent-foreground: oklch(0.205 0 0);
  --sidebar-border: oklch(0.922 0 0);
  --sidebar-ring: oklch(0.708 0 0);
}

.dark {
  --background: oklch(0.145 0 0);
  --foreground: oklch(0.985 0 0);
  --card: oklch(0.205 0 0);
  --card-foreground: oklch(0.985 0 0);
  --popover: oklch(0.205 0 0);
  --popover-foreground: oklch(0.985 0 0);
  --primary: oklch(0.922 0 0);
  --primary-foreground: oklch(0.205 0 0);
  --secondary: oklch(0.269 0 0);
  --secondary-foreground: oklch(0.985 0 0);
  --muted: oklch(0.269 0 0);
  --muted-foreground: oklch(0.708 0 0);
  --accent: oklch(0.269 0 0);
  --accent-foreground: oklch(0.985 0 0);
  --destructive: oklch(0.704 0.191 22.216);
  --success: oklch(0.845 0.143 164.978);
  --warning: oklch(0.905 0.182 98.217);
  --border: oklch(1 0 0 / 10%);
  --input: oklch(1 0 0 / 15%);
  --ring: oklch(0.556 0 0);
  --chart-1: oklch(0.87 0 0);
  --chart-2: oklch(0.556 0 0);
  --chart-3: oklch(0.439 0 0);
  --chart-4: oklch(0.371 0 0);
  --chart-5: oklch(0.269 0 0);
  --sidebar: oklch(0.205 0 0);
  --sidebar-foreground: oklch(0.985 0 0);
  --sidebar-primary: oklch(0.488 0.243 264.376);
  --sidebar-primary-foreground: oklch(0.985 0 0);
  --sidebar-accent: oklch(0.269 0 0);
  --sidebar-accent-foreground: oklch(0.985 0 0);
  --sidebar-border: oklch(1 0 0 / 10%);
  --sidebar-ring: oklch(0.556 0 0);
}
```

### Semantic State

These are first-class because operational state is the product. Each is always paired with an icon and/or text label — never color alone (WCAG 2.2 AA).

- **Positive / Running / Online**: `--success` (emerald-700 light, emerald-300 dark — both ≥4.5:1 on their surfaces) with an icon and explicit label.
- **Warning / Degraded**: `--warning` (amber-700 light, amber-300 dark) with icon and label; hue is secondary to text.
- **Danger / Failed / Error / Offline**: `--destructive`, used sparingly and only for real failure.
- **Idle / Neutral**: muted neutral, the default resting state.

### Named Rules

**The Restraint Rule.** High-contrast primary treatment appears only on primary actions, current selection, focus, and important state. If a surface feels color-heavy, it is wrong.

**The No-Color-Alone Rule.** State is never communicated by color alone. Every status pill carries an icon and/or a text label. This is both a WCAG 2.2 AA commitment and an instrument-console principle: precision tools confirm state twice.

## 3. Typography

**Display Font:** Inter (with `system-ui, -apple-system, Segoe UI, Roboto, sans-serif` fallback)
**Body Font:** Inter (same stack)
**Identifier Font:** a monospace stack (`ui-monospace, SFMono-Regular, Menlo, Consolas, monospace`) reserved for serial numbers, agent IDs, job codes, and machine-readable identifiers.

**Character:** One calm, technical sans carries everything — headings, labels, buttons, body, and dense table data. Inter's open shapes and tabular figures keep operational data legible at small sizes and on Windows/Android. The monospace is not a second voice for contrast; it is a precision tool that flags "this is a machine identifier, you may need to copy it."

### Hierarchy

Fixed rem scale (product UI: users view at consistent DPI; fluid clamp sizes hurt density). Ratio ≈ 1.125–1.2 between steps. [Exact sizes to be resolved during implementation.]

- **Display** (semibold, ~2rem, ~1.15 line-height): page titles; rare inside the app.
- **Headline** (semibold, ~1.5rem): section headings.
- **Title** (medium, ~1.25rem): card and panel titles.
- **Body** (regular, 1rem, 1.5 line-height, capped 65–75ch for prose): default text and table cells.
- **Label** (medium, ~0.8125rem, ~0% tracking, sentence case — not uppercase): field labels, table headers, metadata. Uppercase tracked eyebrows are prohibited.

### Named Rules

**The One-Sans Rule.** Inter is the only proportional family. No display fonts in UI labels, buttons, or data. The monospace appears only for machine identifiers.

**The Fixed-Scale Rule.** Type sizes are fixed rem values, not `clamp()`. A fluid heading that shrinks in a sidebar looks worse, not better, in a dense tool.

## 4. Elevation

Flat by default. Depth is conveyed through tonal layering (white surfaces, neutral secondary/muted layers, and 1px hairlines), not drop shadows. Shadows appear only as a response to state — a hovered/raised element or an open menu — and stay tight (≤8px blur). The "1px border + wide soft shadow" ghost-card pattern is prohibited: pick one treatment per surface. Card corners cap at 12–16px; no over-rounding.

### Named Rules

**The Flat-By-Default Rule.** Surfaces are flat at rest. Shadows are a state response (hover, elevation, focus), never ambient decoration. When a shadow appears, there is no accompanying decorative border on the same element.

## Source of truth

- Status: Active
- Last refreshed: 2026-07-15
- Primary product surfaces: tenant dashboard, Devices, Jobs, Agents, Users, and Settings.
- Evidence reviewed: `frontend/app/dashboard-view-content.tsx`, `frontend/app/dashboard-job-history.tsx`, `frontend/app/dispatch-form.tsx`, `frontend/components/ui`, `frontend/app/globals.css`, and the current Jobs browser screenshot.

## Brand

- Personality: calm, precise, technical, and trustworthy.
- Trust signals: explicit machine state, restrained destructive actions, visible identifiers, and predictable controls.
- Avoid: decorative dashboards, consumer-device novelty, hidden destructive behavior, and color-only status.

## Product goals

- Goals: make fleet state and the next safe operator action immediately scannable; keep common print dispatch work close to job history.
- Non-goals: becoming a slicer, hiding printer state behind decorative summaries, or replacing explicit operator choices with opaque automation.
- Success signals: operators can create a job, verify every required material mapping, and clear finished history without risking an active print.

## Personas and jobs

- Primary personas: self-hosting printer operators and tenant administrators.
- User jobs: monitor printers, dispatch prepared project files, resolve failures, and maintain useful operational history.
- Key contexts of use: repeated desktop use, occasional tablet use, dark rooms/workshops, and degraded network conditions.

## Information architecture

- Primary navigation: Devices, Jobs, Agents, Users, and Settings within the selected tenant.
- Core routes/screens: `/devices`, `/jobs`, `/agents`, `/users`, and `/settings`.
- Content hierarchy: current operational state first, contextual actions second, detailed history and recovery controls after.

## Design principles

- Progressive disclosure: keep creation forms in focused dialogs until requested; reveal metadata and AMS mapping only when the required file and printer context exists.
- Safe operations: destructive collection actions require an explicit confirmation that states what is removed and what is retained.
- Operational density: use compact rows and native controls instead of large cards or wizard steps.
- Tradeoffs: use dialogs for focused creation flows and irreversible confirmations; keep secondary record details inline.

## Visual language

- Color: use the neutral token system above; use destructive color only inside destructive confirmation/action states.
- Typography: use the fixed Inter/system scale and monospace only for machine identifiers.
- Spacing/layout rhythm: use the existing 4px Tailwind rhythm and compact section headers.
- Shape/radius/elevation: rounded-md operational surfaces, hairline borders, and no ambient shadow.
- Motion: short state reveal/focus transitions only; no decorative motion.
- Imagery/iconography: small functional Lucide icons and literal material color swatches.

## Components

- Existing components to reuse: `SectionHeader`, `Button`, `Dialog`, `EmptyState`, native `select`, and the existing dispatch form controls.
- New/changed components: Jobs header action slot; dispatch dialog; compact required-material mapping rows.
- Variants and states: New dialog closed/open; Clear disabled/confirming/clearing; mapping loading/ready/unmapped/no-slots.
- Token/component ownership: shared primitives remain in `frontend/components/ui`; Jobs-specific composition remains in `frontend/app`.

## Accessibility

- Target standard: WCAG 2.2 AA.
- Keyboard/focus behavior: New opens a labelled dialog that traps focus; every material mapping has an accessible select label.
- Contrast/readability: status and material names remain textual; swatches supplement text and never carry meaning alone.
- Screen-reader semantics: use headings, fieldsets, legends, labels, and explicit confirmation descriptions.
- Reduced motion and sensory considerations: no required animation or color-only feedback.

## Responsive behavior

- Supported breakpoints/devices: modern desktop, tablet, and narrow mobile dashboard layouts.
- Layout adaptations: material rows collapse from two columns to one; section header actions wrap without hiding labels.
- Touch/hover differences: all actions remain full buttons/selects with no hover-only capability.

## Interaction states

- Loading: disable the active submit/clear action and retain its context label.
- Empty: keep New available when a tenant exists; disable Clear when no backend-clearable jobs exist.
- Error: keep the current form/dialog context and surface stable error status through the existing action-status system.
- Success: redirect/reconcile the Jobs list and show the existing transient action-status toast.
- Disabled: explain missing tenant, printer, file, AMS slots, or clearable history through nearby copy.
- Offline/slow network: preserve entered dispatch choices; never optimistically remove active job history.

## Content voice

- Tone: concise, literal, and operational.
- Terminology: use Print jobs, New, Clear, required materials, AMS slot, and Unmapped consistently.
- Microcopy rules: destructive confirmation states both the affected count and that active jobs are retained.

## Implementation constraints

- Framework/styling system: Next.js, React, next-intl, Tailwind CSS, and local shadcn-style primitives.
- Design-token constraints: reuse existing semantic CSS variables; do not add a parallel token system.
- Performance constraints: parse 3MF metadata on Hub; calculate the bounded 32-entry visual mapping without browser archive parsing.
- Compatibility constraints: explicit `ams_mapping`/`ams_mapping2` fields remain authoritative and current Agent/Hub protocol shapes are reused.
- Test/screenshot expectations: Vitest interaction coverage, Next build, and manual verification at the current Jobs route.

## Open questions

- [ ] Add reliable slicer nozzle/group metadata to advisory preview before automatically generating `ams_mapping_info` for every dual-nozzle project; owner: print pipeline; impact: nozzle-aware default mapping remains limited to metadata currently exposed by the artifact parser.

## 6. Do's and Don'ts

### Do:

- **Do** keep operational state scannable: status pills always carry an icon + text label, never color alone (WCAG 2.2 AA).
- **Do** reserve high-contrast primary treatment for primary actions, current selection, focus, and important state.
- **Do** use the monospace for serial numbers, agent IDs, and job codes so machine identifiers are visibly distinct and copyable.
- **Do** express tenant and agent boundaries through surface layering and layout, not colored stripes or decorative cards.
- **Do** use a fixed rem type scale and one sans (Inter) across every screen; consistency screen-to-screen is a virtue.
- **Do** ensure body and label text clears 4.5:1 against the slate surface; muted labels still need to be readable.

### Don't:

- **Don't** use decorative SaaS landing-page layouts or oversized hero content inside the app — Pandar is infrastructure software for repeated operational use, not a marketing page.
- **Don't** use playful consumer-device styling, or any visual pattern that obscures operational state.
- **Don't** rely on color alone for state (accessibility and precision both forbid it).
- **Don't** pair a 1px border with a wide soft drop shadow on the same element, or round cards past 16px.
- **Don't** use display fonts, uppercase tracked eyebrows, gradient text, glassmorphism, or side-stripe accent borders.
- **Don't** make a modal the first thought for any interaction; exhaust inline and progressive alternatives first.
- **Don't** add decorative motion. Motion conveys state (change, feedback, loading, reveal) — nothing else.
