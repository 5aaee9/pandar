# Frontend Agent Pairing Guidance Design

## Goal

Improve the Agents page so a tenant administrator can create an agent pairing and understand what to do with the one-time pairing output without leaving the Agents workflow. Operators without pairing permission should still see clear guidance that pairing requires a tenant administrator.

## Current Context

The route `frontend/app/agents/page.tsx` renders the shared dashboard view for `agents`. That view currently shows `LinkedAgentsSection` and `DiagnosticsSection` from `frontend/app/diagnostics-panel.tsx`. Agent pairing creation exists only in admin/settings surfaces through `CreateAgentPairingForm` in `frontend/app/admin-settings-panel.tsx`, and the one-time `agent_env` output is rendered by `SecretActionResult` in `frontend/app/admin-panel-shared.tsx`.

Pandar's product UI direction is a calm operations console. The change must stay dense, scannable, restrained, and action-oriented. It must not add a landing-page-style hero, decorative cards, gradient text, glass effects, or backend behavior.

## Selected Approach

Add an Agents-page pairing guidance section above the linked-agent table.

This is preferable to moving the whole admin panel into Agents or duplicating backend behavior. It keeps pairing creation where tenant administrators naturally look for agents, preserves the existing server action and one-time secret behavior, and keeps tenant/user/token administration in Settings/Users.

Alternative approaches considered:

- Keep pairing only in Settings and add a text pointer from Agents. This is smaller but does not solve the workflow problem because the operator still has to navigate away before creating a pairing.
- Build a full pairing wizard. This would be heavier than the current backend contract requires and risks inventing steps the product does not yet support.

## User Experience

When a tenant is selected and admin resources are available to the current user, the Agents page shows a compact pairing guidance section before the linked-agent table. The section has two columns on desktop and stacks on mobile:

- Left side: a concise title and description explaining that a pairing creates a one-time agent environment block for this tenant.
- Right side: the existing agent-name input and Create pairing button, followed by the one-time output when pairing succeeds.

Below the intro text, the section lists three concrete setup steps:

1. Create a pairing for the selected tenant.
2. Copy the generated environment block into the machine running `pandar-agent`.
3. Start or restart the agent, then use discovery after it appears online.

The copy must be localized in English and Chinese using existing `next-intl` messages. The UI must avoid exposing any persistent secret values beyond the existing one-time result returned by `createAgentPairing`.

When no tenant is selected, the section still appears in a disabled guidance state: it explains that a tenant must be selected before pairing and does not render the form.

When a tenant is selected but `adminUnavailable` is true, the section appears in a restricted guidance state: it explains that only a tenant administrator or an agent-registration-capable principal can create a pairing, and it does not render the form. This scope does not change backend authorization rules.

## Component Structure

Create a focused Agents-page component in `frontend/app/agent-pairing-guidance.tsx`.

Responsibilities:

- Render the pairing guidance section.
- Reuse `CreateAgentPairingForm` for the actual pairing action.
- Render tenant-aware, no-tenant, and restricted states.
- Keep new guidance-section copy in `frontend/messages/en.json` and `frontend/messages/zh.json` under a new `agentPairing` namespace.
- Allow the reused `CreateAgentPairingForm` and `SecretActionResult` to continue using their existing `admin` namespace messages for the form title, input label, button label, pending label, and one-time warning. Do not refactor those shared components only to rename message keys.

Update `frontend/app/dashboard-view-content.tsx` so `AgentsView` passes `selectedTenant` and `adminUnavailable` to the guidance section and renders it above `LinkedAgentsSection`.

Do not change the backend API, server action shape, database schema, or routing.

## Visual Direction

The section should match existing dashboard panels:

- `rounded-md`, `border-slate-300`, white or slate surface, no wide shadow.
- Fixed product type scale; no fluid headings.
- Cyan only for the existing primary action, not decorative accents.
- Monospace only for command/env references.
- No nested cards; use simple grid/flex rows and hairline dividers.

The one-time pairing output remains the amber caution block from `SecretActionResult`, with improved surrounding guidance if needed.

## Accessibility and Responsive Requirements

- The section must have a semantic heading.
- The disabled/no-tenant state must be understandable without relying on color.
- Text must fit at mobile widths without overflowing buttons, inputs, or code blocks.
- Existing keyboard focus behavior must remain visible.
- The generated `agent_env` block must remain horizontally scrollable or wrapping-safe as currently implemented.

## Testing

Add a focused React/Vitest test for the Agents page content behavior.

Test requirements:

- With a selected tenant, the guidance title, setup steps, agent name input, and create button render.
- Without a selected tenant, the guidance title and no-tenant message render, and the agent name input does not render.
- With a selected tenant and `adminUnavailable` set, the restricted message renders, and the agent name input does not render.
- The test must use `NextIntlClientProvider` with `frontend/messages/en.json` so message keys are exercised.

Existing helper tests in `frontend/app/dashboard-shell.test.tsx` remain unchanged unless needed for shared test setup.

## Documentation

Update `docs/roadmap.md` after implementation with a short completed item for the Agents pairing guidance UI and a concise next-step note if relevant.

## Acceptance Criteria

- `/agents` renders a tenant-aware agent pairing guidance section above linked agents.
- Tenant administrators can create an agent pairing from the Agents page using the existing server action.
- The one-time pairing output remains shown only from the server action response and is not persisted by the browser.
- English and Chinese copy are present for all new UI text.
- The no-tenant state gives clear guidance and does not render an unusable pairing form.
- The restricted state gives clear guidance and does not render an unusable pairing form.
- The implementation uses existing product UI vocabulary and avoids decorative patterns.
- Focused frontend tests cover tenant, no-tenant, and restricted rendering.
- Frontend verification passes for the targeted tests and the final repository checks selected in the plan.
