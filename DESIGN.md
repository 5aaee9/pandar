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

- **Positive / Running / Online**: neutral success treatment with an icon and explicit label.
- **Warning / Degraded**: warning treatment with icon and label; hue is secondary to text.
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
