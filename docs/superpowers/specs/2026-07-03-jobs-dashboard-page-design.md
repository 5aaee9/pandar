# Jobs Dashboard Page Design

## Goal

Move job-oriented dashboard controls out of the Devices page into a new Jobs dashboard page.

## Scope

This is a frontend route and layout change only. It does not change Hub APIs, runtime WebSocket behavior, tenant selection, authentication, or job/printer data models.

## Current State

The route-backed dashboard shell exposes `/devices`, `/agents`, `/users`, and `/settings`. The Devices view currently renders the fleet overview, attention list, printer inventory, print job history, dispatch form, and recovery actions in one page.

## Target Behavior

- Add a route-backed `/jobs` dashboard page.
- Add a `Jobs` item to the sidebar navigation.
- Keep tenant query preservation consistent with the other non-Agent pages:
  - Sidebar navigation to Jobs preserves only `tenant`.
  - Tenant switching while on Jobs preserves `tenant` and `status`, but not `command`.
- Change Devices so it renders only:
  - fleet overview status,
  - needs-attention summary/actions,
  - printer inventory.
- Render these existing components on Jobs:
  - `JobHistory`,
  - `DispatchForm`,
  - `RecoveryActions`.
- Keep the existing dashboard data-loading path and live runtime merge behavior. Jobs uses the same loaded `jobs`, `printers`, and `agents` data as Devices previously used.
- Pass action status through the dashboard runtime query state so sidebar tenant switching from Jobs can preserve `status`.
- Keep job-page actions on the Jobs page after completion. Dispatch upload, refresh-all/refresh-agent controls rendered by `RecoveryActions`, retry dispatch, bulk retry, reprint, duplicate, and live printer controls should redirect back to `/jobs?...&status=...` when submitted from Jobs.
- Preserve existing non-Jobs action behavior. Devices/Needs-attention actions and AMS refresh should continue to redirect to `/devices?...&status=...`; Agents/admin/user routes keep their current redirects.
- Add English and Chinese sidebar labels for Jobs.
- Update `docs/roadmap.md` after implementation.

## Acceptance Criteria

- `/devices` no longer renders the headings `Print jobs`, `Dispatch print job`, or `Recovery actions`.
- `/devices` still renders the fleet status area, needs-attention region when applicable, and `Printer inventory`.
- `/jobs` renders `Print jobs`, `Dispatch print job`, and `Recovery actions`.
- The sidebar includes a Jobs link and highlights it when `/jobs` is active.
- `DASHBOARD_VIEWS` includes `jobs`, and dashboard URL helper tests cover `/jobs` URL generation.
- Sidebar navigation to Jobs preserves `tenant` while dropping `command` and `status`.
- Tenant switching on Jobs preserves `status` while dropping `command`.
- Runtime/sidebar integration tests cover tenant switching on Jobs through the rendered sidebar, not just direct helper calls.
- Job-page action tests cover redirects to `/jobs` for dispatch upload and recovery actions submitted with the Jobs return marker.
- Existing Devices/Needs-attention action tests continue to cover default redirects to `/devices`.
- English messages include `Jobs`; Chinese messages include `任务`.
- Existing frontend tests pass with the new route behavior.

## Safety and Rollback

The change is isolated to frontend routing/view composition and translation/docs files. Rollback is a normal git revert of the commit.
