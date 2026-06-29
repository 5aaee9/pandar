# Dashboard Shell Vitest React Migration Design

## Goal

Migrate the dashboard shell standalone smoke test into the root frontend Vitest suite and React Testing Library so dashboard route helpers and the tenant switcher interaction are covered by the normal frontend test command.

## Scope

In scope:

- Replace `frontend/app/dashboard-shell.smoke.mjs` with Vitest coverage in the root `frontend/` app.
- Preserve the exact pure-helper assertions from the standalone smoke test for dashboard views, title keys, root redirects, sidebar URLs, tenant-switch URLs, and logout href selection.
- Add React Testing Library coverage for `DashboardShellHeader` to verify tenant switching uses `dashboardTenantHref` through `window.location.assign`.
- Keep the test under `frontend/app/` near the dashboard shell code and run it with `npm --prefix frontend run test -- app/dashboard-shell.test.tsx`.
- Update `docs/roadmap.md` after the code change.
- Remove direct documentation references that instruct developers to run the deleted dashboard shell smoke script.

Out of scope:

- Migrating the other standalone smoke scripts under `frontend/app/`.
- Changing dashboard shell runtime behavior, route paths, query semantics, localization text, or sidebar UI.
- Adding backend APIs, database changes, or new dependencies.
- Changing the root `frontend/package.json` test script, which already runs `vitest run`.

## Current Context

The root frontend already has Vitest, jsdom, and React Testing Library configured through `frontend/vitest.config.ts`, `frontend/vitest.setup.ts`, and `frontend/package.json`. Existing root app React tests live beside app code, for example `frontend/app/onboarding-access.test.tsx`.

The standalone smoke file `frontend/app/dashboard-shell.smoke.mjs` imports `frontend/app/dashboard-shell.ts` with Node's TypeScript stripping support and asserts the pure route helper contract. That smoke file is outside the standard `npm --prefix frontend run test` path and does not exercise React components.

`frontend/app/dashboard-shell-header.tsx` renders the tenant selector inside the dashboard shell header. The selector calls `window.location.assign(dashboardTenantHref(view, tenantId, query))` when a tenant is chosen.

## Design

Create `frontend/app/dashboard-shell.test.tsx` using Vitest, React Testing Library, `@testing-library/user-event`, and `NextIntlClientProvider` with `frontend/messages/en.json`.

The test file contains two groups:

- Pure helper tests copied from `dashboard-shell.smoke.mjs`, converted from Node `assert` calls to Vitest `expect` assertions.
- A React interaction test that renders `DashboardShellHeader` with two tenants, opens the tenant selector by changing the select value, and verifies `window.location.assign` receives the expected `/agents?tenant=t2&command=cmd1&status=done` URL when the current view is `agents` and the query contains tenant, command, and status.

The React test should mock only framework boundaries that are not relevant to the behavior under test:

- Mock `next/navigation`'s `useRouter` with a `refresh` spy so `LanguageSwitcher` can render.
- Mock the sidebar context trigger only if the real `SidebarTrigger` requires provider state outside this focused header test. If a mock is needed, keep it limited to `../components/ui/sidebar` and render a plain button for `SidebarTrigger`.

Delete `frontend/app/dashboard-shell.smoke.mjs` after the Vitest test demonstrates the same helper contract. Do not keep a compatibility wrapper or duplicate smoke script.

Documentation updates:

- Update `docs/roadmap.md` Completed section with a short note that dashboard shell smoke coverage now runs through Vitest/React Testing Library.
- Update the historical dashboard sidebar spec/plan references only where they are active run instructions for `frontend/app/dashboard-shell.smoke.mjs`; do not rewrite old design history beyond avoiding a dangling deleted-file command.

## Acceptance Criteria

- `frontend/app/dashboard-shell.test.tsx` exists and is discovered by Vitest.
- `frontend/app/dashboard-shell.smoke.mjs` is removed.
- The Vitest helper assertions cover all assertions previously present in `frontend/app/dashboard-shell.smoke.mjs`:
  - `DASHBOARD_VIEWS` equals `['devices', 'agents', 'users', 'settings']`.
  - `dashboardViewTitleKey` returns each view key.
  - `dashboardRootRedirectTarget` routes empty query to `/devices`, preserves tenant/status on devices, and routes command queries to `/agents` with tenant, command, and status.
  - `dashboardSidebarHref` preserves tenant only and drops command/status.
  - `dashboardTenantHref` preserves tenant/status and preserves command only for the `agents` view.
  - `logoutHref` returns `null` for no sign-out URL and the configured sign-out URL otherwise.
- The React test verifies selecting another tenant in `DashboardShellHeader` calls `window.location.assign` with the URL produced for the current dashboard view and query semantics.
- `npm --prefix frontend run test -- app/dashboard-shell.test.tsx` exits 0.
- `npm --prefix frontend run test` exits 0.
- `npm --prefix frontend run build` exits 0, or any failure is captured exactly if it is caused by the environment or pre-existing issue.
- Rust repository verification required by `AGENTS.md` is run after edits: `cargo fmt`, `cargo clippy`, and `cargo nextest run --manifest-path "Cargo.toml" --workspace`. If a command fails because tooling or environment is missing, record the exact failure.

## Safety and Rollback

This is a test-only migration plus documentation cleanup. The production dashboard code should not change. Rollback is a normal git revert that restores the standalone smoke script and removes the Vitest test.

The main risk is mocking too much and weakening coverage. Keep mocks limited to Next.js routing/sidebar provider boundaries and assert real exported helper functions plus the real `DashboardShellHeader` tenant select behavior.

## Documentation Impact

Update `docs/roadmap.md` after implementation to record the completed migration. Avoid creating a new user-facing runbook because this is an internal test harness change and the existing `frontend` test script remains the public command.
