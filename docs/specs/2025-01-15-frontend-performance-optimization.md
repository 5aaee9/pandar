# Spec: Frontend Performance Optimization (Phase 1)

## Background

Pandar's frontend currently ships 35 client components in a single bundle with limited code splitting. The `dashboard-data.tsx` file is already a server component that fetches data, but all view components (`DashboardRuntime`, `DashboardViewContent`, etc.) are client components. This results in a large initial JS payload and poor perceived performance on slower networks.

## Goals

1. **Add Suspense boundaries and loading.tsx** to all dashboard routes for immediate visual feedback
2. **Implement code splitting** for heavy components (DispatchForm, DiagnosticsSection) via dynamic imports
3. **Convert static content to React Server Components** where possible to reduce client-side JavaScript

## Non-Goals

- Virtualization of long lists (deferred to Phase 2)
- Zustand state management (deferred to Phase 2)
- E2E test coverage (deferred to Phase 3)
- Bundle size reduction below 800KB (aggressive target, Phase 2)

## Requirements

### R1: Route-Level Loading States

**R1.1** Each dashboard route (`/devices`, `/jobs`, `/agents`, `/users`, `/settings`) must have a `loading.tsx` file.

**R1.2** Loading states must use the existing `Skeleton` component from `@/components/ui/skeleton`.

**R1.3** Loading states must match the layout structure of the actual page content area (not the shared shell). Specifically:
- Route-local section header with title/subtitle skeleton (not a duplicate shell header)
- Content area with 2-4 skeleton cards/rows
- No sidebar skeleton (sidebar is part of shared shell)

**R1.4** Loading states must respect reduced motion preferences (no pulse animation when `prefers-reduced-motion` is set).

**R1.5** Loading states must replace only route content, not the shared dashboard shell (sidebar, header). This requires restructuring:
- **Shell ownership**: `DashboardRuntime` currently owns the shell (sidebar, header). Split into:
  - `DashboardShellProvider` (new client component, owns shell state, event subscriptions, navigation)
  - `DashboardShellLayout` (new client component, renders shell UI, receives `children` as route content)
- **Route group**: Create `frontend/app/(dashboard)/layout.tsx` (new file, server component) that:
  - Fetches shared data (tenants, auth) from `dashboard-data.tsx` utilities
  - Passes shared data to `DashboardShellProvider`
  - Renders `DashboardShellProvider` and `DashboardShellLayout`
  - Accepts `children` as route content
- **Route pages**: Move route pages into `frontend/app/(dashboard)/devices/page.tsx`, `frontend/app/(dashboard)/jobs/page.tsx`, etc. Each page:
  - Reads `searchParams` to determine selected tenant, command, status
  - Fetches route-specific data via `dashboard-data.tsx` utilities
  - Passes route data to client components
- **Loading files**: Create `frontend/app/(dashboard)/devices/loading.tsx`, `frontend/app/(dashboard)/jobs/loading.tsx`, etc. that render only content skeletons
- **Implicit Suspense**: `loading.tsx` files provide implicit Suspense boundaries (no additional explicit `<Suspense>` required)

**R1.6** Data ownership:
- Shared data (tenants, auth): Fetched by `(dashboard)/layout.tsx` server component
- Selected tenant: Determined by each route page from `searchParams` (not layout)
- Route data: Fetched by each route page server component:
  - `/devices`: printers, agents, jobs
  - `/jobs`: jobs, printers, agents (for dispatch), membership roles (for `canManageJobs`)
  - `/agents`: agents, printers (for diagnostics), command results, membership roles (for `adminUnavailable`)
  - `/users`: users, user identities, join links, membership roles (for `adminUnavailable`)
  - `/settings`: tenant tokens, agents, printers (for TenantSettingsStatic), audit events, membership roles (for `adminUnavailable`)
- Tenant list access: Route pages access tenant list via request-memoized server utility `getTenantsForRequest()` (new function in `dashboard-data.tsx`) that caches tenant list per request (no duplicate API calls)
- Membership roles: Route pages access membership roles via request-memoized server utility `getMembershipForRequest(tenantId)` (new function in `dashboard-data.tsx`) that caches membership per request (no duplicate API calls)

**R1.7** Error ownership:
- Shared data errors: Handled by `(dashboard)/layout.tsx` (redirect to login or show error)
- Route data errors: Handled by each route page (pass error state as props to client components)

**R1.8** Loading state visibility: When navigating between routes with prefetch disabled, loading state appearance is measured as a target (observational, not gating). Measurement procedure:
- Open Chrome DevTools Performance tab
- Start recording
- Navigate from `/settings` to `/devices` via `<Link prefetch={false}>` sidebar link
- Stop recording when loading skeleton appears
- Measure time from navigation start (click event) to first paint of loading skeleton
- Repeat 5 times, use median
- Environment: Chrome 120+, Fast 3G throttling profile (400ms RTT, 400kbps down, 400kbps up), production build, cold cache
- Prefetch disabled: Use `<Link prefetch={false}>` for sidebar links
- Target: 500ms (document actual timing in `docs/roadmap.md`)

**R1.9** Shell event ownership: `DashboardShellProvider` owns event subscriptions and exposes client context:
- `DashboardShellProvider` provides `DashboardShellContext` (new client context) with `registerRouteData` and `unregisterRouteData` methods
- Route pages render `DashboardRouteRegistrar` (new client component) that consumes `DashboardShellContext` and calls `registerRouteData` on mount, `unregisterRouteData` on unmount
- When route changes, `DashboardRouteRegistrar` unregisters old data, registers new data, `DashboardShellProvider` re-initializes subscriptions
- Unregister token-awareness: `unregisterRouteData(token)` only clears registration if token matches active registration (prevents stale unregisters from clearing newer data)
- Registration token: Each registration receives opaque token (UUID) from `DashboardShellProvider`, used for unregister and update matching (prevents stale cleanup on same-route refresh)
- Obsolete subscription handling: `DashboardShellProvider` cancels obsolete subscriptions on new registration, ignores updates from cancelled subscriptions
- Same-route refresh: On same-route refresh (tenant change), old registration unregisters with old token, new registration registers with new token (no stale cleanup)
- Registration payload: `RouteRegistration` shape:
  ```tsx
  type RouteRegistration = {
    token: string // UUID from DashboardShellProvider
    view: DashboardView
    tenant: Tenant | null
    command: string | null
    status: string | null
    errors: string[]
    actionStatus: string | null
    initialPrinters: Printer[]
    initialJobs: Job[]
  }
  ```
- Example:
  ```tsx
  // (dashboard)/layout.tsx (server component)
  <DashboardShellProvider>
    <DashboardShellLayout>{children}</DashboardShellLayout>
  </DashboardShellProvider>

  // (dashboard)/devices/page.tsx (server component)
  <DashboardRouteRegistrar
    selectedTenant={tenant}
    printers={printers}
    jobs={jobs}
    view="devices"
  />
  <DashboardRouteConsumer>
    <DashboardViewContent
      view="devices"
      initialPrinters={printers}
      initialJobs={jobs}
      ...
    /> {/* client component */}
  </DashboardRouteConsumer>
  ```

**R1.10** Selected tenant resolution: Route pages determine selected tenant as follows:
- Read `searchParams.tenant` (string or string[])
- Call `getTenantsForRequest()` to get tenant list (request-memoized, no duplicate API calls)
- If valid tenant ID exists in tenants list, use it
- If invalid or absent, use first tenant in list (or null if empty)
- Selected tenant type: `Tenant | null`
- Default behavior: First tenant in list
- Invalid behavior: Ignore invalid value, use first tenant
- Preserve existing behavior: Handle `APP_TENANT_ID` (synthesize tenant), external provider tenants (from `/api/v1/me`), onboarding (empty tenants), redirects, and membership roles

**R1.11** Live route state ownership: `DashboardShellProvider` owns live route state (printers, jobs) and exposes it via `DashboardShellContext`:
- `DashboardShellContext` provides `livePrinters: Printer[]`, `liveJobs: Job[]`, `liveView: DashboardView | null`, `liveTenantId: string | null`
- Route pages render `DashboardRouteConsumer` (new client component) that consumes `DashboardShellContext` and passes live data to route views
- Initial render: Route views receive `initialPrinters` and `initialJobs` props from server, render immediately (no empty state)
- Subscription updates: When subscriptions receive updates, `DashboardShellProvider` updates context, `DashboardRouteConsumer` re-renders with new data
- Tenant scoping: `DashboardRouteConsumer` checks `liveTenantId` matches current `selectedTenant.id`, ignores stale data from previous tenant
- Stale registration cleanup: `DashboardRouteRegistrar` unregisters on unmount, `DashboardShellProvider` clears live state
- Registration identity: `DashboardRouteRegistrar` registers with unique key `${view}:${selectedTenant?.id ?? 'none'}`, `DashboardShellProvider` tracks active registration, clears on unregister or new registration

**R1.12** Tenant navigation: Shell tenant selector updates `searchParams` via `useRouter`:
- Tenant selector in `DashboardShellLayout` calls `router.push(`/devices?tenant=${tenantId}`)` (or current view)
- Sidebar links preserve tenant via `<Link href={`/devices?tenant=${tenantId}`}>` (or current tenant)
- Route data refetches: When `searchParams.tenant` changes, route page re-renders, fetches new data, `DashboardRouteRegistrar` registers new data

**R1.13** Settings data freshness: `TenantSettingsStatic` receives server-fetched printer props for initial render. Live printer updates are preserved via client island: `TenantSettingsStatic` accepts `livePrintersSlot: ReactNode` prop that renders a client component (`TenantSettingsLivePrinters`) which consumes `DashboardShellContext` for real-time printer updates. This preserves existing live-update behavior while keeping static layout in RSC.

**R1.14** Route-dependent shell state: `DashboardShellProvider` receives route-dependent state (view, selected tenant, command, status, errors, action status) via `DashboardRouteRegistrar` registration:
- First render: Shell renders with client-safe initial state (view from `usePathname()`, tenant from `useSearchParams()`), route content renders immediately
- Post-hydration: `DashboardRouteRegistrar` registers route data, `DashboardShellProvider` updates shell state
- Context contract: `DashboardShellContext` provides `shellView: DashboardView`, `shellTenant: Tenant | null`, `shellCommand: string | null`, `shellStatus: string | null`, `shellErrors: string[]`, `shellActionStatus: string | null`
- Client-safe initial state: `DashboardShellProvider` uses `usePathname()` and `useSearchParams()` to determine initial state on client (no server parsing, no hydration mismatch)

**R1.15** Tenant/auth freshness: Layout tenant list is synchronized with page tenant list via server revalidation:
- Mutation owners: Server actions in `frontend/app/actions.ts` (tenant creation, membership updates) and `frontend/app/admin-actions.ts` (tenant deletion) call `revalidatePath('/(dashboard)', 'layout')` after mutations
- Refresh triggers: Tenant creation, membership role change, tenant deletion
- Loop prevention: Revalidation only triggers on explicit mutations (not on route data changes)
- Onboarding: Layout renders `OnboardingPanel` inline when tenants list is empty (existing behavior, no redirect)
- Auth refresh: Layout handles auth token refresh via existing `authSource()` pattern
- Membership changes: Server actions call `revalidatePath('/(dashboard)', 'layout')` after membership updates (role changes, tenant creation)

### R2: Component-Level Code Splitting

**R2.1** `DispatchForm` must be lazy-loaded via `next/dynamic` when accessed from `/jobs` route. Since `DispatchForm` is nested in `DispatchDialog` (which opens on user interaction), the chunk will load when the dialog opens (not on route navigation). This is acceptable for Phase 1 (dialog is not immediately visible). **Note**: `DispatchDialog` is shared by Devices and Jobs routes, so lazy loading affects both routes (global lazy loading).

**R2.2** `DiagnosticsSection` (from `diagnostics-panel.tsx`) must be extracted into separate module `frontend/app/diagnostics-section.tsx` (new file) and lazy-loaded via `next/dynamic` when accessed from `/agents` route (current location), loading on route navigation (not first interaction). Extraction required because `diagnostics-panel.tsx` also exports `LinkedAgentsSection` (statically imported by `AgentsView`).

**R2.3** Lazy-loaded components must show a `Skeleton` fallback during loading.

**R2.4** Code splitting must not break existing functionality (all existing tests must pass, plus new tests for lazy loading behavior).

**R2.5** SSR must remain enabled for lazy-loaded components (no `ssr: false`).

**R2.6** Exclusion from initial route chunk must be verified via network request analysis:
- **Hard load**: Open Chrome DevTools Network tab, navigate directly to `/jobs` or `/agents` (not client navigation):
  - `/jobs`: Verify `DispatchForm` chunk is NOT requested on initial page load (only when dialog opens)
  - `/agents`: Verify `DiagnosticsSection` chunk is requested during initial page load (hydration chunk, not entry chunk)
- **Client navigation**: Navigate from `/settings` to `/jobs` or `/agents` via sidebar link:
  - `/jobs`: Verify `DispatchForm` chunk is NOT requested during route navigation (only when dialog opens)
  - `/agents`: Verify `DiagnosticsSection` chunk is requested during route navigation (not on first interaction)
- **Interaction trigger**: For `/jobs`, verify `DispatchForm` chunk loads when dialog opens (not on initial page load or route navigation)
- **Distinct chunk**: Verify chunk appears as separate file in `.next/static/chunks/` (name may be hashed, not component name)

### R3: React Server Components Migration

**R3.1** **Settings page static sections**: Extract the following into separate server component files:
- `SettingsStaticPanels` in `frontend/app/settings-static-panels.tsx` (new file, RSC):
  - Props: `languageSwitcher: ReactNode`, `themeSwitcher: ReactNode`
  - Renders: `LanguageSettingsPanel` and `ThemeSettingsPanel` static layout (section headers, descriptions)
  - Slots: `languageSwitcher` and `themeSwitcher` passed as children (client components)
- `TenantSettingsStatic` in `frontend/app/tenant-settings-static.tsx` (new file, RSC):
  - Props: `tenant: Tenant | null`, `agents: Agent[]`, `printers: Printer[]`, `auth: AuthMetadata`, `livePrintersSlot: ReactNode`
  - Renders: `TenantSettings` static layout (section headers, DetailGroup/DetailLine static text)
  - Slots: `livePrintersSlot` (client component `TenantSettingsLivePrinters` that consumes `DashboardShellContext` for live printer updates, **mandatory for settings route**)
  - Auth context: `auth` prop passed from route page server component (which reads from cookies/headers)
  - No interactive elements (all data passed as props)
  - Note: Live printer updates preserved via client island (`TenantSettingsLivePrinters`)

**R3.2** **Users page static sections**: Extract the following into separate server component files:
- `UsersStaticPanels` in `frontend/app/users-static-panels.tsx` (new file, RSC):
  - Props: `usersTable: ReactNode`, `emptyState: ReactNode`
  - Renders: `UsersAdminSection` static layout (section header, empty state)
  - Slots: `usersTable` (client component with interactive forms), `emptyState` (static EmptyState)
  - Note: Route page server component decides which slot to render based on `users.length`:
    ```tsx
    // (dashboard)/users/page.tsx (server component)
    <UsersStaticPanels
      usersTable={users.length > 0 ? <UsersTable ... /> : null}
      emptyState={users.length === 0 ? <EmptyState ... /> : null}
    />
    ```

**R3.3** **Composition**: RSC components are rendered by route page server components (`(dashboard)/settings/page.tsx`, `(dashboard)/users/page.tsx`) and passed as props to client components (`DashboardViewContent`). Example:
```tsx
// (dashboard)/settings/page.tsx (server component)
<DashboardViewContent
  settingsStaticPanels={
    <SettingsStaticPanels
      languageSwitcher={<LanguageSwitcher />}
      themeSwitcher={<ThemeSwitcher />}
    />
  }
  tenantSettingsStatic={
    <TenantSettingsStatic
      tenant={tenant}
      agents={agents}
      printers={printers}
      auth={auth}
      livePrintersSlot={<TenantSettingsLivePrinters />}
    />
  }
  ...
/>
```

**R3.4** **DashboardViewContent slot contract**: `DashboardViewContent` (client component) must accept new props:
- `settingsStaticPanels?: ReactNode` (RSC component for settings page)
- `tenantSettingsStatic?: ReactNode` (RSC component for settings page)
- `usersStaticPanels?: ReactNode` (RSC component for users page)

**R3.5** **Data ownership**: RSC components receive data via props from route page server components. No direct API calls in RSC components.

**R3.6** **Auth context**: RSC components access tenant/auth context via props passed from route page server components (which read from cookies/headers). No direct cookie access in RSC components.

**R3.7** **Locale propagation**: RSC components use `useTranslations` from `next-intl` (server-side). Locale is determined by route page server component via `getLocale()`.

**R3.8** **Error handling**: Route page server components handle errors by passing error states as props to client components (existing pattern). No new `error.tsx` boundaries required (errors are already handled in `dashboard-data.tsx` via `fetchJson` error returns). API failures show existing `role="alert"` error banner (not EmptyState).

**R3.9** **Data freshness**: Route page server components fetch data with `cache: 'no-store'` to ensure real-time dashboard data (existing pattern).

**R3.10** **RSC verification**: RSC components must be verified as server-only by:
- Absence of `'use client'` directive in the file
- Manual code review confirming the module is not imported into any client component graph
- Server-side rendering verification (no hydration errors in console)

**R3.11** **Old static path removal**: Old static client-render paths in `dashboard-admin-views.tsx` must be removed (not just supplemented) to guarantee bundle reduction. Specifically:
- Remove `LanguageSettingsPanel` and `ThemeSettingsPanel` static layout from `dashboard-admin-views.tsx` (moved to `SettingsStaticPanels`)
- Remove `TenantSettings` static layout from `dashboard-runtime-sections.tsx` (moved to `TenantSettingsStatic`)
- Remove `UsersAdminSection` static layout from `dashboard-admin-views.tsx` (moved to `UsersStaticPanels`)

### R4: Bundle Size Measurement

**R4.1** **Goal**: Measure bundle size reduction for `/settings` route after Phase 1 changes.

**R4.2** **Baseline**: Total size of initial JS chunks for `/settings` route after `npm --prefix frontend run build` on implementation branch merge-base, compressed via `gzip -9` per file, summed. Baseline stored in `scripts/bundle-baseline.json` (placeholder values, must be regenerated):
```json
{
  "settings": {
    "js": 1331200,
    "css": 98304
  },
  "devices": { "js": 0, "css": 0 },
  "jobs": { "js": 0, "css": 0 }
}
```

**R4.3** **Target**: Same measurement on feature branch. **No mandatory reduction threshold** (measurement only). Document actual reduction in `docs/roadmap.md`. If reduction is 0% or negative, document reason and defer further optimization to Phase 2.

**R4.4** **Measurement procedure**:
- Script: `scripts/measure-bundle.sh`
- Two commands:
  - `./scripts/measure-bundle.sh --generate-baseline <commit>`: Creates isolated git worktree at `<commit>`, runs `npm --prefix frontend install && npm --prefix frontend run build` in worktree, measures bundle sizes, writes to `scripts/bundle-baseline.json` in caller's checkout, cleans up worktree
  - `./scripts/measure-bundle.sh --compare`: Runs `npm --prefix frontend run build`, measures bundle sizes, compares against `scripts/bundle-baseline.json`, outputs reduction percentage, returns nonzero if CSS constraint violated
- Baseline must be generated from the implementation branch's merge-base (canonical ref: `git merge-base HEAD main`)
- Parses `frontend/.next/app-build-manifest.json` (Next.js 16.2.10 App Router manifest) to identify initial JS chunks for `/settings` route
- Parses `frontend/.next/react-loadable-manifest.json` (Next.js 16.2.10 dynamic import manifest) to identify lazy chunks
- CSS measurement: Scans `frontend/.next/static/css/` directory for all CSS files, sums gzipped sizes (no route-specific CSS in Next.js 16.2.10 App Router)
- Resolves chunk paths relative to `frontend/.next/`
- Deduplicates shared chunks (root layout, route group layout, page chunks)
- Measures initial JS and CSS sizes (gzipped, summed)
- Compares against baseline stored in `scripts/bundle-baseline.json`
- Outputs reduction percentage and absolute bytes
- Worktree cleanup: Script must remove worktree after baseline generation (success or failure)
- Dependency installation: Script must run `npm --prefix frontend install` in worktree before build
- Zero-valued entries: `devices` and `jobs` entries in baseline are placeholders (measured but not used for comparison)

**R4.5** **Environment**: Node 24, production build (`NODE_ENV=production`), cold cache.

**R4.6** **CSS bundle**: Must not increase (Tailwind classes must be reused). CSS size measured via `frontend/.next/static/css/` directory scan, compressed via `gzip -9` per file, summed.

**R4.7** **Expected savings source**:
- Code splitting: `DispatchForm` (~50KB) and `DiagnosticsSection` (~30KB) moved to separate chunks
- RSC migration: Static panels (~20KB) moved to server components
- Total expected savings: ~100KB (~8% of 1.3MB)
- **Note**: Additional optimizations (tree shaking, dead code elimination) may be required for larger reductions. Defer to Phase 2 if needed.

### R5: Backward Compatibility

**R5.1** All existing routes must remain accessible (no URL changes).

**R5.2** Auth flow must be unchanged (login, logout, token refresh).

**R5.3** Locale switching must be unchanged (en/zh).

**R5.4** Persisted settings must be unchanged (theme, language, sidebar state).

**R5.5** All existing functionality must work identically (no feature regressions), verified via:
- All 385 existing tests pass
- Manual testing of critical flows (login, device creation, job dispatch, settings update) - **non-gating** (requires external services)
- New tests for `?tenant=` navigation and route-data refresh

## Acceptance Criteria

- [ ] All 5 dashboard routes have `loading.tsx` files
- [ ] Route group layout `frontend/app/(dashboard)/layout.tsx` created and renders `DashboardShellProvider` and `DashboardShellLayout`
- [ ] `DashboardRuntime` split into `DashboardShellProvider` (state/events) and `DashboardShellLayout` (UI)
- [ ] `DashboardRouteRegistrar` created and registers route data with `DashboardShellProvider` via client context
- [ ] `DashboardRouteConsumer` created and consumes live route state from `DashboardShellContext`
- [ ] `DiagnosticsSection` extracted into `frontend/app/diagnostics-section.tsx` (new file)
- [ ] `DispatchForm` and `DiagnosticsSection` are lazy-loaded and appear in separate chunks (verified via network request analysis)
- [ ] `SettingsStaticPanels`, `TenantSettingsStatic`, and `UsersStaticPanels` are RSC (verified via absence of `'use client'` directive, manual code review confirming no client component imports, and server-side rendering verification)
- [ ] Old static client-render paths removed from `dashboard-admin-views.tsx` and `dashboard-runtime-sections.tsx`
- [ ] Bundle size reduction measured for `/settings` route (verified via `scripts/measure-bundle.sh --compare`), actual reduction documented
- [ ] CSS bundle size unchanged or reduced (verified via `scripts/measure-bundle.sh --compare`)
- [ ] All existing tests pass (385 tests)
- [ ] New tests added for lazy loading behavior (2-4 tests, assert `Skeleton` fallback renders and resolved content renders)
- [ ] New tests added for `?tenant=` navigation and route-data refresh (2-4 tests)
- [ ] New tests added for stale unregisters, obsolete subscription updates, tenant transitions, and initial-data fallback (4-6 tests)
- [ ] Loading states match page content layout (verified via visual inspection at 375px, 768px, 1280px viewports)
- [ ] No accessibility regressions (focus management preserved, verified via keyboard navigation: Tab, Shift+Tab, Enter, Esc)
- [ ] No auth/locale/settings regressions (verified via manual testing: login, logout, locale switch, theme switch) - **non-gating** (requires external services)
- [ ] Error handling preserved (API failures show existing `role="alert"` error banner, no stack traces or credentials exposed)

## Safety & Rollback

- **Rollback strategy**: All changes (R1, R2, R3) must be committed as one atomic unit and squash-merged into main. Rollback via single `git revert <squash-commit>` of the squash merge. R1, R2, R3 share modified files (`dashboard-data.tsx`, `dashboard-runtime.tsx`, `dashboard-view-content.tsx`) and cannot be reverted independently without conflicts.
- No database migrations required
- No API changes required
- No feature flags required (all changes are structural, not behavioral)
- Server-to-Hub reachability: Parent server components must handle Hub API failures gracefully (existing pattern via `fetchJson` error returns, no new error handling required)

## Documentation Impact

- Update `docs/roadmap.md` with completion status and actual bundle size reduction
- Update `DESIGN.md` with:
  - Loading state pattern (mandatory, as 5 routes will use the same pattern)
  - Provider/context architecture (`DashboardShellProvider`, `DashboardRouteRegistrar`, `DashboardRouteConsumer`)
  - Route-data ownership model (layout vs page responsibilities)

## Dependencies

- Next.js 16.2.10 (existing, per `frontend/package.json`)
- React 19.2.3 (existing)
- No new npm packages

## Manual Verification Prerequisites

- Backend Hub API running on `http://localhost:8080` (or `APP_API_URL` env var)
- Unauthenticated test user (for auth-disabled verification: `APP_AUTH_PROVIDER=none`, `APP_API_TOKEN=demo-token`)
- Test data: at least one printer, one agent, one job
- Chrome DevTools with 3G throttling profile
- Cold cache (disable cache in DevTools Network tab)

## Validation Commands

```bash
# Run all tests
npm --prefix frontend run test

# Type check
npm --prefix frontend run typecheck

# Lint
npm --prefix frontend run lint

# Build and measure bundle size
npm --prefix frontend run build

# Generate baseline (run on merge-base commit)
./scripts/measure-bundle.sh --generate-baseline <merge-base-commit>

# Compare against baseline (run on feature branch)
./scripts/measure-bundle.sh --compare

# Rust checks (repository-mandated)
cargo fmt
cargo clippy
cargo nextest run --manifest-path Cargo.toml --workspace

# Manual verification with production build
npm --prefix frontend run start
```

## Manual Verification Checklist

**Mandatory checks** (gating):
- [ ] Verify lazy-loaded components load correctly (no broken UI, no console errors)
- [ ] Verify RSC components render correctly (no hydration errors in console)
- [ ] Verify keyboard navigation works (Tab, Shift+Tab, Enter, Esc)
- [ ] Verify reduced motion preference is respected (no pulse animation)
- [ ] Verify dark mode works correctly
- [ ] Verify error handling works (stop Hub API, verify error banner shows)
- [ ] Verify lazy chunks load correctly:
  - `/jobs`: `DispatchForm` chunk loads when dialog opens (not on initial page load or route navigation)
  - `/agents`: `DiagnosticsSection` chunk loads on route navigation (not on first interaction)
- [ ] Verify `?tenant=` navigation works (change tenant via URL, verify selected tenant updates)
- [ ] Verify route-data refresh works (create new job, verify jobs list updates)
- [ ] Verify tenant-scoped live state (change tenant, verify live printers/jobs update to new tenant, no stale data from previous tenant)
- [ ] Verify settings live printer updates work (change printer status, verify settings page updates in real-time)
- [ ] Verify layout freshness after tenant/membership mutations (create tenant, update role, verify layout tenant list updates)
- [ ] Verify inline onboarding renders when tenants list is empty (no redirect)
- [ ] Verify initial shell state on SSR (view from URL path, tenant from `?tenant=` param, no hydration mismatch)

**Observational checks** (non-gating):
- [ ] Navigate from `/settings` to `/devices` with Fast 3G throttling (400ms RTT, 400kbps), verify loading state appears (document actual timing in `docs/roadmap.md`)
- [ ] Verify locale switching works (en/zh)

**Conditional checks** (non-gating, require external services):
- [ ] Verify auth flow works (login, logout) with `APP_AUTH_PROVIDER=logto` (requires Logto endpoint and credentials)
- [ ] Verify device creation works (requires Hub API and printer)
- [ ] Verify job dispatch works (requires Hub API, printer, and 3MF file)
- [ ] Verify settings update works (requires Hub API and tenant admin role)

## Out of Scope

- Image optimization (no images in current UI)
- Font optimization (system fonts only)
- Service worker / PWA features
- Internationalization lazy loading (all locales bundled)
- Virtualization of long lists
- Zustand state management
- E2E test coverage
