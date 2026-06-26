<!-- SEED: re-run $impeccable document once design tokens live in CSS to capture the actual values, tonal ramps, and component CSS. -->

---
name: Pandar
description: Calm, technical operations console for self-hosted Bambu printer infrastructure.
---

# Design System: Pandar

## 1. Overview

**Creative North Star: "The Instrument Console"**

Pandar is infrastructure software for repeated operational use, not a marketing page. It should feel like a precision instrument panel: dense, scannable, quiet at rest, and loud only when state demands attention. The operator glances at it many times a day to confirm health and act on exceptions, so the interface must disappear into the task. Trust comes from precision and consistency, never from decoration.

The system is **Restrained**. A single teal/cyan accent carries primary actions, the current selection, and active state indicators — and almost nothing else. Surfaces are cool, near-neutral slate, with a slightly cooler second layer for sidebars and panels so tenant and agent boundaries read structurally rather than decoratively. Typography is one well-tuned sans (Inter) at a fixed rem scale, with a monospace reserved for machine identifiers — serial numbers, agent IDs, job codes — because tabular, identifiable data is the substance of this product.

This system explicitly rejects decorative SaaS landing-page layouts, oversized hero content inside the app, playful consumer-device styling, and any visual pattern that obscures operational state. If a choice makes the dashboard feel like a consumer gadget or a marketing site, it is wrong.

**Key Characteristics:**
- Calm and technical; trustworthy by precision, not by ornament.
- Operational state is scannable at a glance; exceptions break the calm deliberately.
- Restrained accent (teal/cyan) used for action, selection, and state — never decoration.
- Monospace for machine identifiers; one sans for everything else.
- Explicit tenant and agent boundaries expressed through layout and surface layering.
- Density is a virtue; the same visual vocabulary recurs screen to screen.

## 2. Colors

The palette is a cool, near-neutral slate base with a single teal/cyan accent. Neutrals are tinted very slightly toward the accent hue (chroma ≈ 0.005–0.015) so surfaces feel like one system rather than generic gray. The accent appears on ≤10% of any screen.

### Primary
- **Console Teal** (oklch ≈ 0.50 0.11 200, near the existing `#0e7490` selection anchor) [exact ramp to be resolved during implementation]: primary actions, current selection, active focus, and the "running / online" positive state. Used sparingly — its rarity is the point.

### Neutral
- **Slate Surface** [values to be resolved during implementation]: the base content background, continuing the existing `#f1f5f9` slate-100 direction but tuned for ≥4.5:1 body-text contrast.
- **Panel Slate** [values to be resolved during implementation]: a slightly cooler/darker layer for sidebars, toolbars, and panels, so tenant/agent boundaries read as structure.
- **Ink** [values to be resolved during implementation]: body and heading text. Dark enough that muted labels still clear 4.5:1 against the slate surface — light gray "for elegance" is prohibited.
- **Hairline** [values to be resolved during implementation]: 1px borders and dividers only.

### Semantic State (named roles; exact values to be resolved during implementation)
These are first-class because operational state is the product. Each is always paired with an icon and/or text label — never color alone (WCAG 2.2 AA).
- **Positive / Running / Online**: the Console Teal accent.
- **Warning / Degraded**: a warm amber, reserved for attention-that-isn't-failure.
- **Danger / Failed / Error / Offline**: a firm red, used sparingly and only for real failure.
- **Idle / Neutral**: slate, the default resting state.

### Named Rules
**The Restraint Rule.** The accent appears on ≤10% of any given screen. It is reserved for primary actions, the current selection, focus, and the running/online state. If a surface feels "teal-heavy," it is wrong.

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

Flat by default. Depth is conveyed through tonal layering (Panel Slate vs Slate Surface) and 1px hairlines, not drop shadows. Shadows appear only as a response to state — a hovered/raised element or an open menu — and stay tight (≤8px blur). The "1px border + wide soft shadow" ghost-card pattern is prohibited: pick one treatment per surface. Card corners cap at 12–16px; no over-rounding.

### Named Rules
**The Flat-By-Default Rule.** Surfaces are flat at rest. Shadows are a state response (hover, elevation, focus), never ambient decoration. When a shadow appears, there is no accompanying decorative border on the same element.

## 6. Do's and Don'ts

### Do:
- **Do** keep operational state scannable: status pills always carry an icon + text label, never color alone (WCAG 2.2 AA).
- **Do** reserve the teal/cyan accent for primary actions, current selection, focus, and running/online state — ≤10% of any screen.
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
