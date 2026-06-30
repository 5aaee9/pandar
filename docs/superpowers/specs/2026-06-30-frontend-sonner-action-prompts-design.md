# Frontend Sonner Action Prompts Design

## Scope

Replace transient dashboard action prompts with shadcn Sonner toasts. This covers action feedback currently carried by the `status` query parameter and rendered as the cyan inline banner in `DashboardRuntime`, including examples such as `refresh_queued`, `job_created`, `retry_queued`, `reprint_queued`, `duplicate_queued`, `printer_control_queued`, partial success statuses, tenant/user/link/token status redirects, and backend error-code statuses.

This does not replace durable in-page operational content:

- The live activity notification feed in `RuntimeStatusPanel` remains a persistent dashboard panel because it is historical runtime context, not a transient prompt.
- Admin secret action results remain in-page because they include generated tokens or agent environment output that must stay visible long enough to copy.
- Fetch/data integrity errors remain in-page because they describe degraded dashboard content rather than completed user actions.

## Current Behavior

Server actions redirect back to dashboard routes with `?status=<code>`. `renderDashboardView` reads `status` into `actionStatus`, and `DashboardRuntime` renders it in an inline cyan banner using `runtime.actionStatus` translations when available, otherwise a token-prettified fallback. The `status` query remains in the URL, so refreshing or changing dashboard tenants can keep the same prompt visible.

## Target Behavior

The app mounts a Sonner `Toaster` once inside the root layout, following the shadcn Sonner component pattern from `https://ui.shadcn.com/docs/components/radix/sonner`.

When `DashboardRuntime` receives an `actionStatus`, a client-only toast is shown after hydration. The inline cyan banner is removed. The toast content uses localized `runtime.actionStatus` messages for all known redirect statuses and falls back to the same readable token formatting used today only for unexpected backend error codes.

After the toast fires, the consumed status is removed from both the current browser URL and dashboard navigation state. `DashboardRuntime` keeps a local consumed-action-status state derived from the server-rendered `actionStatus`; after consumption it builds `dashboardQuery` with no `status`, so tenant-switch/navigation hrefs rendered after consumption do not preserve the old status. The browser URL is replaced with the same path and all existing query parameters except `status`. This raw `window.history.replaceState` call is intentional because dashboard links are driven by `DashboardRuntime` props/local state after clearance rather than by `useSearchParams()`. This prevents repeated toasts on refresh or tenant switching while preserving `tenant`, `command`, and any other navigation context.

Toast tone is selected from the status code:

- Warning: if the status contains `partial`.
- Error: if the status starts with `http_` or is not one of the known positive redirect statuses.
- Success/default: all known positive redirect statuses, including queued, created, updated, revoked, linked, accepted, and related action outcomes.

React Strict Mode development double-invocation must not produce duplicate toasts for the same status in one mount. The toast trigger should guard already-consumed statuses before calling Sonner.

## Components and Data Flow

- Add the Sonner package and local shadcn-style wrapper at `frontend/components/ui/sonner.tsx`.
- Import `Toaster` into `frontend/app/layout.tsx` and render it inside `NextIntlClientProvider` so every dashboard route can emit toasts.
- Add a small client component or helper in the dashboard runtime area that receives `actionStatus`, resolves the localized message with `next-intl`, calls `toast`, records that the status has been consumed, and removes only the `status` query parameter with `window.history.replaceState`.
- Keep server actions redirecting with `status` codes. This preserves the current server action flow and avoids redesigning form submissions.
- Keep `DashboardQuery.status` support while the server render passes the status into the client. The client clears it after display and passes a status-free query to dashboard links after consumption.
- Add `runtime.actionStatus` entries in English and Chinese for the known status redirects emitted by the frontend server actions, so success examples such as `refresh_queued` are localized instead of fallback-prettified. Known redirect statuses are `refresh_queued`, `refresh_partial`, `job_created`, `tenant_created`, `tenant_token_revoked`, `join_link_accepted`, `join_link_revoked`, `user_created`, `user_role_updated`, `identity_linked`, `retry_queued`, `retry_partial`, `reprint_queued`, `duplicate_queued`, and `printer_control_queued`.

## Acceptance Criteria

- The dashboard no longer renders the old inline cyan action-status banner.
- Visiting `/devices?tenant=t1&status=refresh_queued` shows a Sonner toast with localized text equivalent to "Refresh Queued" and then changes the URL to `/devices?tenant=t1` without a full page navigation.
- Visiting `/devices?tenant=t1&command=c1&status=refresh_partial` shows a warning toast and preserves `tenant=t1&command=c1` when removing `status`.
- Visiting a backend error status such as `artifact_too_large` shows an error toast with readable fallback text if no translation exists.
- Switching tenants after the toast fires does not reintroduce the consumed status into the destination URL or show the same toast again.
- Live activity notifications, admin secret results, and fetch/data errors remain visible in their existing in-page locations.
- English and Chinese locales resolve all known redirect statuses from frontend actions, and unexpected backend error codes still use readable fallback text.

## Testing

- Add or update frontend unit tests around action status toast behavior, using mocked Sonner `toast` functions and a real `window.location`/history environment under Vitest.
- Keep existing dashboard shell URL-building tests passing.
- Run `cd frontend && npm run test`.
- Run `cd frontend && npm run build` as the production smoke check.

## Docs Impact

Update `docs/roadmap.md` after implementation to record that redirect-backed dashboard action prompts now use Sonner toasts instead of the inline status banner.

## Self-Review

- No placeholders or TBDs remain.
- The scope separates transient action prompts from durable runtime notifications and secret result panels.
- The URL-clearing behavior is explicit and preserves non-status query parameters.
- The test and docs expectations are concrete enough for an implementation plan.
