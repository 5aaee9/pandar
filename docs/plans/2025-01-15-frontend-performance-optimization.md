# Plan: Frontend Performance Optimization (Phase 1)

## Overview

Implement route-level loading states, component code splitting, and React Server Components migration to reduce initial bundle size and improve perceived performance.

## Prerequisites

- Reviewed spec: `docs/specs/2025-01-15-frontend-performance-optimization.md`
- Current commit: `9a478dd`
- Branch: `main`
- Merge-base for baseline: `9a478dd` (pin before implementation, record in `scripts/bundle-baseline.json`)
- Pre-change baseline: Run existing 385 tests and record results before implementation

## Tasks

### Task 0: Create Bundle Measurement Script and Generate Baseline

**Files**:
- `scripts/measure-bundle.sh` (new)
- `scripts/bundle-baseline.json` (new, generated)

**Actions**:
1. Create `scripts/measure-bundle.sh` with `--generate-baseline` and `--compare` commands:
   - `--generate-baseline <commit>`: Creates isolated git worktree at `<commit>`, runs `npm --prefix frontend install && npm --prefix frontend run build` in worktree, measures bundle sizes, writes to `scripts/bundle-baseline.json` in caller's checkout, cleans up worktree (success or failure)
   - `--compare`: Runs `npm --prefix frontend run build`, measures bundle sizes, compares against `scripts/bundle-baseline.json`, outputs reduction percentage, returns nonzero if CSS constraint violated
2. Script enforces environment: Node 24, production build (`NODE_ENV=production`), cold cache
3. Script parses `.next/app-build-manifest.json` to identify initial JS chunks for `/settings` route
4. Script parses `.next/react-loadable-manifest.json` to identify lazy chunks
5. Script scans `.next/static/css/` for CSS files, sums gzipped sizes via `gzip -9`
6. Script deduplicates shared chunks (root layout, route group layout, page chunks), resolves paths relative to `frontend/.next/`
7. Baseline generated from pinned merge-base: `9a478dd` (recorded in prerequisites)
8. Baseline entries: All routes (`settings`, `devices`, `jobs`) are measured and recorded (devices/jobs used for future comparison)
9. Fail-closed: Script fails if expected baseline or post-migration manifest keys are absent (no misleading successful comparison)

**Validation**:
- `./scripts/measure-bundle.sh --generate-baseline 9a478dd` (before implementation)
- `./scripts/measure-bundle.sh --compare` (after implementation)

### Task 1: Add Request-Memoized Server Utilities

**Files**:
- `frontend/app/dashboard-data.tsx` (modified)

**Actions**:
1. Add `getTenantsForRequest()` function that caches tenant list per request using React `cache()`:
   ```tsx
   import { cache } from 'react'
   export const getTenantsForRequest = cache(async () => {
     // existing tenant fetch logic, returns { tenants: Tenant[], error: string | null }
   })
   ```
2. Add `getIdentityForRequest()` function that caches identity per request using React `cache()`:
   ```tsx
   export const getIdentityForRequest = cache(async () => {
     // existing /api/v1/me fetch logic, returns { me: MeResponse | null, error: string | null, status: number | null }
   })
   ```
3. Add `getMembershipForRequest(tenantId: string)` function that caches membership per request using React `cache()` (authoritative source for membership roles, derives from `getIdentityForRequest()`):
   ```tsx
   export const getMembershipForRequest = cache(async (tenantId: string) => {
     const { me, error: identityError } = await getIdentityForRequest()
     if (identityError) {
       return { role: null, error: identityError }
     }
     const membership = me?.tenants.find((t) => t.tenant_id === tenantId)
     return { role: membership?.role ?? null, error: null }
   })
   ```
4. Add `getAuthForRequest()` function that caches auth per request using React `cache()` (wraps existing `authSource()`, returns complete `AuthMetadata` including provider URLs)
5. Add `resolveEffectiveTenants(tenants, identity, configuredTenantId, authProvider)` function that normalizes effective tenant list (external identity tenants + synthesized `APP_TENANT_ID` + deduplication):
   ```tsx
   export function resolveEffectiveTenants(tenants: Tenant[], identity: MeResponse | null, configuredTenantId: string | undefined, authProvider: string): Tenant[] {
     // If configuredTenantId, return synthesized tenant
     // If authProvider === 'none', return fetched tenants
     // If external onboarding, return identity tenants
     // Otherwise, merge fetched tenants with identity tenants, deduplicate by id
   }
   ```
6. Add `resolveSelectedTenant(searchParams, effectiveTenants)` function that handles:
   - String arrays (take first value)
   - Invalid IDs (fallback to first tenant)
   - Empty tenants (return null)
   - External tenants (already normalized in effectiveTenants)
   - `APP_TENANT_ID` (already synthesized in effectiveTenants)
6. Add typed route loaders:
   ```tsx
   export async function loadDevicesRoute(tenantId: string): Promise<{ printers: Printer[], agents: Agent[], jobs: Job[], error: string | null }>
   export async function loadJobsRoute(tenantId: string): Promise<{ jobs: Job[], printers: Printer[], agents: Agent[], error: string | null }>
   export async function loadAgentsRoute(tenantId: string, commandId: string | null): Promise<{ agents: Agent[], printers: Printer[], command: Command | null, commandData: CommandData | null, error: string | null }>
   export async function loadUsersRoute(tenantId: string): Promise<{ users: User[], identities: UserIdentity[], joinLinks: JoinLink[], adminError: string | null }>
   export async function loadSettingsRoute(tenantId: string): Promise<{ tenantTokens: TenantToken[], agents: Agent[], printers: Printer[], auditEvents: AuditEvent[], adminError: string | null }>
   ```
   - All loaders use `cache: 'no-store'` for real-time data
   - All loaders return typed result shapes with error field
   - `loadAgentsRoute` includes `parseCommandResult` for command data (returns `CommandResultData`)
   - `loadUsersRoute` and `loadSettingsRoute` use `adminError` for administrative endpoint failures (users, join-links, tenant-tokens, audit-events)
   - Membership roles fetched separately via `getMembershipForRequest()` (not in loaders)
7. Export functions for use in route pages
8. Preserve existing behavior: onboarding, redirects, membership roles

**Validation**:
- `npm --prefix frontend run typecheck`
- `npm --prefix frontend run lint`

### Task 2: Create Dashboard Shell Provider and Layout

**Files**:
- `frontend/app/dashboard-shell-provider.tsx` (new)
- `frontend/app/dashboard-shell-layout.tsx` (new)

**Actions**:
1. Create `DashboardShellProvider` client component that:
   - Props: `initialTenants: Tenant[]`, `initialAuth: AuthMetadata`, `sidebarDefaultOpen: boolean`, `apiUrl: string`
   - Owns shell state, event subscriptions, navigation
   - Provides `DashboardShellContext` with:
     - `registerRouteData(registration: RouteRegistration): string` (returns UUID token)
     - `unregisterRouteData(token: string): void` (token-aware, only clears if token matches active registration)
     - `livePrinters: Printer[]`, `liveJobs: Job[]`, `liveView: DashboardView | null`, `liveTenantId: string | null`
     - `shellView: DashboardView`, `shellTenant: Tenant | null`, `shellCommand: string | null`, `shellStatus: string | null`, `shellErrors: string[]`, `shellActionStatus: string | null`
     - `notifications: RuntimeNotification[]`, `liveState: LiveState`, `lastEventAt: string | null`
     - `actionToast: ActionToast | null`, `errorBanner: string | null`
   - Uses `usePathname()` and `useSearchParams()` for initial state
   - Resolves initial `shellTenant` from `useSearchParams().get('tenant')` and `initialTenants` prop
   - Cancels obsolete subscriptions on new registration, ignores updates from cancelled subscriptions
   - Clears live state on unregister or new registration
   - Owns clock state for live updates
2. Create `DashboardShellLayout` client component that:
   - Props: `sidebarDefaultOpen: boolean`, `tenants: Tenant[]`, `auth: AuthMetadata`
   - Renders shell UI (sidebar, header, `<main>` frame)
   - Renders `ActionStatusToast` and `role="alert"` error banner from `DashboardShellContext` (`shellActionStatus`, `shellErrors`)
   - Accepts `children` as route content
   - Uses `<Link prefetch={false}>` for sidebar links
   - Preserves selected tenant in sidebar links via `<Link href={`/devices?tenant=${tenantId}`}>`
   - Tenant selector uses `router.push(`${currentPath}?tenant=${tenantId}`)` where `currentPath` from `usePathname()` (preserves current route, updates `?tenant=`)
   - Absent or invalid tenant parameter selects first effective tenant (R1.10)

**Validation**:
- `npm --prefix frontend run typecheck`
- `npm --prefix frontend run lint`

### Task 3: Create Route Registrar and Consumer

**Files**:
- `frontend/app/dashboard-route-registrar.tsx` (new)
- `frontend/app/dashboard-route-consumer.tsx` (new)

**Actions**:
1. Create `DashboardRouteRegistrar` client component that:
   - Consumes `DashboardShellContext`
   - Calls `registerRouteData` on mount with `RouteRegistrationInput` payload:
     ```tsx
     type RouteRegistrationInput = {
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
   - Receives UUID token from `registerRouteData` (provider generates token, returns string)
   - Calls `unregisterRouteData(token)` on unmount
   - **Re-registers on prop change**: Uses `useEffect` with scalar dependencies `[view, tenant?.id, command, status, actionStatus]` and memoized arrays `errors`, `initialPrinters`, `initialJobs` (via `useMemo`) to unregister old token and register new token when props change
   - Registration identity: `${view}:${tenant?.id ?? 'none'}` (used for debugging, token is authoritative for unregister/update matching)
   - Subscription condition: Only subscribes when `view !== 'users'` (preserves existing behavior)
2. Create `DashboardRouteConsumer` client component that:
   - Props: `view: DashboardView`, `selectedTenant: Tenant | null`, `initialPrinters: Printer[]`, `initialJobs: Job[]`, `routeData: RouteData`, `rscSlots: RscSlots`, `auth: AuthMetadata`
   - Consumes `DashboardShellContext`
   - Renders `DashboardViewContent` with live data (merges live printers/jobs with route-specific data and RSC slots)
   - Checks `liveTenantId` matches current `selectedTenant.id`, ignores stale data from previous tenant
   - Falls back to `initialPrinters` and `initialJobs` props before subscription updates
   - Forwards route-specific data, RSC slots, and auth to `DashboardViewContent`
   - Derives `canManageJobs` (auth.provider === 'none' || routeData.membership?.role !== 'viewer'), `adminUnavailable` (auth.provider !== 'none' && (routeData.membership?.role !== 'tenant_admin' || routeData.membership?.error !== null || routeData.adminError !== null)), `adminLoadError` (auth.provider !== 'none' && routeData.membership?.role === 'tenant_admin' && (routeData.membership?.error !== null || routeData.adminError !== null))
   - Assembles errors before registration (membership errors, admin errors, route errors), passes to `DashboardRouteRegistrar` for `shellErrors` context
   - Derives health/attention from routeData agents + live printers/jobs (agents from routeData, single owner)
   - Does NOT render `DashboardRouteRegistrar` (route pages render registrar as sibling, single registration owner)
   - Type definitions:
     ```tsx
     type RouteData = 
       | { view: 'devices', printers: Printer[], agents: Agent[], jobs: Job[], error: string | null }
       | { view: 'jobs', jobs: Job[], printers: Printer[], agents: Agent[], membership: { role: string | null, error: string | null }, error: string | null }
       | { view: 'agents', agents: Agent[], printers: Printer[], command: Command | null, commandData: CommandResultData | null, membership: { role: string | null, error: string | null }, error: string | null }
       | { view: 'users', users: User[], identities: UserIdentity[], joinLinks: JoinLink[], membership: { role: string | null, error: string | null }, adminError: string | null }
       | { view: 'settings', tenantTokens: TenantToken[], agents: Agent[], printers: Printer[], auditEvents: AuditEvent[], membership: { role: string | null, error: string | null }, adminError: string | null }
     type RscSlots = {
       settingsStaticPanels?: ReactNode
       tenantSettingsStatic?: ReactNode
       usersStaticPanels?: ReactNode
     }
     ```

**Validation**:
- `npm --prefix frontend run typecheck`
- `npm --prefix frontend run lint`

### Task 4: Create RSC Static Panels

**Files**:
- `frontend/app/settings-static-panels.tsx` (new)
- `frontend/app/tenant-settings-static.tsx` (new)
- `frontend/app/users-static-panels.tsx` (new)
- `frontend/app/tenant-settings-live-printers.tsx` (new)
- `frontend/app/dashboard-admin-views.tsx` (modified)

**Actions**:
1. Create `SettingsStaticPanels` RSC component that:
   - Props: `languageSwitcher: ReactNode`, `themeSwitcher: ReactNode`
   - Renders: `LanguageSettingsPanel` and `ThemeSettingsPanel` static layout (section headers, descriptions)
   - Uses `useTranslations` from `next-intl` (server-side)
2. Create `TenantSettingsStatic` RSC component that:
   - Props: `tenant: Tenant | null`, `agents: Agent[]`, `printers: Printer[]`, `auth: AuthMetadata`, `livePrintersSlot: ReactNode`
   - Renders: `TenantSettings` static layout (section headers, DetailGroup/DetailLine static text)
   - Renders `livePrintersSlot` in place of old printer list (mandatory, no duplicate content)
   - Uses `useTranslations` from `next-intl` (server-side)
3. Create `UsersStaticPanels` RSC component that:
   - Props: `usersPanel: ReactNode`, `emptyState: ReactNode`
   - Renders: `UsersAdminSection` static layout (section header, empty state)
   - Wraps content in `AdminSectionGuard` for authorization/error behavior (existing component, no changes required)
   - Uses `useTranslations` from `next-intl` (server-side)
4. Create `TenantSettingsLivePrinters` client component that:
   - Props: `initialPrinters: Printer[]`, `selectedTenant: Tenant | null`
   - Consumes `DashboardShellContext`
   - Renders initial printers from props (server-fetched, immediate render)
   - Transitions to live printers from context when subscription updates arrive (tenant-scoped, no duplicate content)
5. Update `SettingsView` in `dashboard-admin-views.tsx` to accept `settingsStaticPanels` and `tenantSettingsStatic` props, render them when provided
6. Update `UsersView` in `dashboard-admin-views.tsx` to accept `usersStaticPanels` prop, render it when provided

**Validation**:
- `npm --prefix frontend run typecheck`
- `npm --prefix frontend run lint`
- `npm --prefix frontend run test -- --run app/dashboard-shell.test.tsx`

### Task 5: Verify Sidebar Navigation

**Files**:
- `frontend/components/app-sidebar.tsx` (no changes required)

**Actions**:
1. Verify `frontend/components/app-sidebar.tsx` already uses `prefetch={false}` and tenant-preserving helpers (no changes required)

**Validation**:
- Manual review

### Task 6: Extract DiagnosticsSection for Code Splitting

**Files**:
- `frontend/app/diagnostics-section.tsx` (new)
- `frontend/app/diagnostics-panel.tsx` (modified)
- `frontend/app/dashboard-view-content.tsx` (modified)

**Actions**:
1. Create `frontend/app/diagnostics-section.tsx` with `DiagnosticsSection` component (extracted from `diagnostics-panel.tsx`)
2. Modify `diagnostics-panel.tsx` to export only `LinkedAgentsSection`
3. Modify `dashboard-view-content.tsx` to lazy-load `DiagnosticsSection` via `next/dynamic` with `Skeleton` fallback (SSR enabled, no `ssr: false`)

**Validation**:
- `npm --prefix frontend run typecheck`
- `npm --prefix frontend run lint`
- `npm --prefix frontend run test -- --run app/dashboard-view-content.test.tsx`

### Task 7: Lazy-Load DispatchForm

**Files**:
- `frontend/app/dispatch-dialog.tsx` (modified)

**Actions**:
1. Modify `dispatch-dialog.tsx` to lazy-load `DispatchForm` via `next/dynamic` with `Skeleton` fallback (SSR enabled, no `ssr: false`)

**Validation**:
- `npm --prefix frontend run typecheck`
- `npm --prefix frontend run lint`
- `npm --prefix frontend run test -- --run app/dispatch-form.test.tsx`

### Task 8: Create Route Group Layout Structure

**Files**:
- `frontend/app/(dashboard)/layout.tsx` (new)
- `frontend/app/(dashboard)/devices/page.tsx` (new)
- `frontend/app/(dashboard)/jobs/page.tsx` (new)
- `frontend/app/(dashboard)/agents/page.tsx` (new)
- `frontend/app/(dashboard)/users/page.tsx` (new)
- `frontend/app/(dashboard)/settings/page.tsx` (new)
- `frontend/app/(dashboard)/devices/loading.tsx` (new)
- `frontend/app/(dashboard)/jobs/loading.tsx` (new)
- `frontend/app/(dashboard)/agents/loading.tsx` (new)
- `frontend/app/(dashboard)/users/loading.tsx` (new)
- `frontend/app/(dashboard)/settings/loading.tsx` (new)
- `frontend/app/devices/page.tsx` (deleted)
- `frontend/app/jobs/page.tsx` (deleted)
- `frontend/app/agents/page.tsx` (deleted)
- `frontend/app/users/page.tsx` (deleted)
- `frontend/app/settings/page.tsx` (deleted)
- `frontend/app/dashboard-view-content.tsx` (modified)

**Actions**:
1. Create `frontend/app/(dashboard)/layout.tsx` server component that:
   - Fetches tenants via `getTenantsForRequest()` (skip if external onboarding or `APP_TENANT_ID` configured), identity via `getIdentityForRequest()` (skip if auth.provider === 'none'), auth via `getAuthForRequest()`
   - Calls `resolveEffectiveTenants(tenants, identity, configuredTenantId, auth.provider)` to get effective tenant list (handles external onboarding, APP_TENANT_ID, auth-disabled mode)
   - Reads `sidebar_state` cookie via `dashboardSidebarDefaultOpen()`
   - Passes shared data to `DashboardShellProvider` (`initialTenants` (effective), `initialAuth`, `sidebarDefaultOpen`, `apiUrl`)
   - Renders `DashboardShellProvider` and `DashboardShellLayout`
   - Accepts `children` as route content
   - Handles shared-data errors (redirect to login or show error)
   - Handles auth redirects (existing `dashboardAuthRedirectTarget` logic, uses `getIdentityForRequest().status`)
   - Renders `OnboardingPanel` inline when effective tenants list is empty AND auth.provider !== 'none' AND identity exists (external-auth onboarding, not auth-disabled empty tenant list), replaces shell and route children (existing conditional behavior)
2. Create route page server components that:
   - Read `searchParams` to determine selected tenant, command, status
   - Call `getTenantsForRequest()` (skip if external onboarding or `APP_TENANT_ID` configured) to get tenants
   - Call `getIdentityForRequest()` (skip if auth.provider === 'none') to get identity
   - Call `resolveEffectiveTenants(tenants, identity, configuredTenantId, auth.provider)` to get effective tenant list
   - Call `resolveSelectedTenant(searchParams, effectiveTenants)` to get selected tenant
   - Call `getMembershipForRequest(tenantId)` to get membership roles (if tenantId is not null AND auth.provider !== 'none', authoritative source, skip if auth-disabled)
   - Pass membership and auth to `DashboardRouteConsumer` (single derivation owner)
   - Call `getAuthForRequest()` to get auth
   - Call `getLocale()` to get locale
   - Fetch route-specific data via typed route loaders (skip if tenantId is null, return typed empty results):
     - `/devices`: `loadDevicesRoute(tenantId)`
     - `/jobs`: `loadJobsRoute(tenantId)`
     - `/agents`: `loadAgentsRoute(tenantId, commandId)` where `commandId` from `searchParams.command`
     - `/users`: `loadUsersRoute(tenantId)`
     - `/settings`: `loadSettingsRoute(tenantId)`
   - Pass route data to client components
   - Render `DashboardRouteRegistrar` and `DashboardRouteConsumer`
   - Render RSC static panels for settings/users routes:
     - Settings: `<SettingsStaticPanels languageSwitcher={<LanguageSwitcher />} themeSwitcher={<ThemeSwitcher />} />`, `<TenantSettingsStatic tenant={tenant} agents={agents} printers={printers} auth={auth} livePrintersSlot={<TenantSettingsLivePrinters initialPrinters={printers} selectedTenant={tenant} />} />`
     - Users: `<UsersStaticPanels usersPanel={users.length > 0 ? <TenantUsersPanel ... /> : null} emptyState={users.length === 0 && !adminError ? <EmptyState ... /> : null} />`
   - Handle route-data errors (pass error state as props to client components, existing `role="alert"` banner)
3. Create `loading.tsx` files that render route-local skeletons (header + content, no sidebar, 2-4 route-matching rows/cards using existing `Skeleton` component)
4. Modify `dashboard-view-content.tsx` to accept RSC slot props (`settingsStaticPanels`, `tenantSettingsStatic`, `usersStaticPanels`)
5. Delete old route pages: `frontend/app/devices/page.tsx`, `frontend/app/jobs/page.tsx`, etc. (atomic cutover, same task)

**Validation**:
- `npm --prefix frontend run typecheck`
- `npm --prefix frontend run lint`
- `npm --prefix frontend run build`

### Task 9: Remove Old Static Paths and Runtime

**Files**:
- `frontend/app/dashboard-admin-views.tsx` (modified)
- `frontend/app/dashboard-runtime-sections.tsx` (modified)
- `frontend/app/dashboard-runtime.tsx` (deleted)
- `frontend/app/dashboard-data.tsx` (modified)

**Actions**:
1. Remove `LanguageSettingsPanel` and `ThemeSettingsPanel` static layout from `dashboard-admin-views.tsx`
2. Remove `TenantSettings` static layout from `dashboard-runtime-sections.tsx`
3. Remove `UsersAdminSection` static layout from `dashboard-admin-views.tsx`
4. Delete `frontend/app/dashboard-runtime.tsx` (no backward compatibility wrapper, per no-legacy-fallback constraint)
5. Delete `renderDashboardView` from `dashboard-data.tsx` (replaced by route pages)
6. Verify root route (`frontend/app/page.tsx`) still works (no changes required, redirects preserved)

**Validation**:
- `npm --prefix frontend run typecheck`
- `npm --prefix frontend run lint`
- `npm --prefix frontend run test -- --run app/dashboard-shell.test.tsx app/dashboard-view-content.test.tsx`

### Task 10: Add Revalidation to Server Actions

**Files**:
- `frontend/app/admin-actions.ts` (modified)

**Actions**:
1. Inventory all tenant-creation, membership-change, and tenant-deletion actions:
   - Tenant creation: `createTenantFromExternal` in `admin-actions.ts`
   - Membership changes: `acceptJoinLink` in `admin-actions.ts`, `updateTenantUserRole` in `admin-actions.ts`
   - Tenant deletion: None exists (verify before adding)
2. Add `revalidatePath('/(dashboard)', 'layout')` after successful mutations but before throwing redirects:
   - `createTenantFromExternal` in `admin-actions.ts` (tenant creation)
   - `acceptJoinLink` in `admin-actions.ts` (membership updates)
   - `updateTenantUserRole` in `admin-actions.ts` (membership updates)

**Validation**:
- `npm --prefix frontend run typecheck`
- `npm --prefix frontend run lint`

### Task 11: Add Automated Tests

**Files**:
- `frontend/app/dashboard-runtime.test.tsx` (deleted or updated)
- `frontend/app/dashboard-shell.test.tsx` (modified)
- `frontend/app/dashboard-route-registrar.test.tsx` (new)
- `frontend/app/dashboard-route-consumer.test.tsx` (new)
- `frontend/app/dashboard-data.test.tsx` (modified)
- `frontend/app/dispatch-dialog.test.tsx` (new)
- `frontend/app/dashboard-view-content.test.tsx` (modified)
- `frontend/app/dashboard-shell-provider.test.tsx` (new)
- `frontend/app/dashboard-layout.test.tsx` (new)
- `frontend/app/admin-actions.test.tsx` (new)
- `frontend/app/tenant-settings-live-printers.test.tsx` (new)

**Actions**:
- Tests are added alongside their corresponding implementation tasks (not as a separate phase):
  - Task 1: Memoization tests (getTenantsForRequest, getIdentityForRequest, getMembershipForRequest, getAuthForRequest), selected-tenant edge cases (string arrays, invalid IDs, empty tenants, external tenants, APP_TENANT_ID), auth-disabled authorization behavior, auth-disabled mode (no identity request), external-auth mode (no general tenants endpoint), configured-tenant behavior in `dashboard-data.test.tsx`
  - Task 2: Obsolete subscription updates (updates from cancelled subscriptions are ignored) in `dashboard-shell-provider.test.tsx`
  - Task 3: Stale unregisters (unregister with old token doesn't clear new registration), tenant transitions (live state clears on tenant change), initial-data fallback (route views render with initial data before subscription updates), registrar prop-change re-registration in `dashboard-route-registrar.test.tsx` and `dashboard-route-consumer.test.tsx`
  - Task 4: Tenant-scoped initial fallback in `tenant-settings-live-printers.test.tsx`
  - Tasks 6-7: Lazy loading behavior (Skeleton fallback renders, resolved content renders) in `dispatch-dialog.test.tsx` and `dashboard-view-content.test.tsx`
  - Task 8: `?tenant=` navigation and route-data refresh, layout/page integration (auth redirects, inline onboarding, route error propagation, unchanged URLs) in `dashboard-shell.test.tsx` and `dashboard-layout.test.tsx`
  - Task 9: Migrate `dashboard-runtime.test.tsx` behavioral assertions to `dashboard-shell-provider.test.tsx` and `dashboard-layout.test.tsx` (all 385 existing tests must pass, no optional deletion)
  - Task 10: Successful-versus-failed revalidation calls in `admin-actions.test.tsx`

**Validation**:
- `npm --prefix frontend run test`

### Task 12: Measure Fast 3G Loading-Skeleton Timing

**Actions**:
1. Run production build: `npm --prefix frontend run build`
2. Start production server: `npm --prefix frontend run start`
3. Navigate from `/settings` to `/devices` with Fast 3G throttling (400ms RTT, 400kbps) 5 times
4. Measure loading-skeleton timing (median of 5 runs)
5. Record timing in `docs/roadmap.md`

**Validation**:
- Timing recorded

### Task 13: Update Documentation

**Files**:
- `docs/roadmap.md` (modified)
- `DESIGN.md` (modified)

**Actions**:
1. Run bundle comparison: `./scripts/measure-bundle.sh --compare`
2. Update `docs/roadmap.md` with completion status, actual bundle size reduction (from comparison), and Fast 3G loading-skeleton timing (median of 5 runs, from Task 12)
3. Update `DESIGN.md` with loading state pattern, provider/context architecture, route-data ownership model

**Validation**:
- Manual review

### Task 14: Final Verification

**Actions**:
1. Run all tests: `npm --prefix frontend run test`
2. Run typecheck: `npm --prefix frontend run typecheck`
3. Run lint: `npm --prefix frontend run lint`
4. Run build: `npm --prefix frontend run build`
5. Run bundle measurement: `./scripts/measure-bundle.sh --compare`
6. Run Rust checks: `cargo fmt`, `cargo clippy`, `cargo nextest run --manifest-path Cargo.toml --workspace`
7. Manual verification (mandatory checks):
   - Visual validation at 375px, 768px, 1280px viewports against PRODUCT.md/DESIGN.md
   - Keyboard navigation (Tab, Shift+Tab, Enter, Esc)
   - Reduced motion preference
   - Dark mode
   - Error handling (stop Hub API, verify error banner shows)
   - Lazy chunk loading (network request analysis):
     - Hard load: `/jobs` (DispatchForm not requested), `/agents` (DiagnosticsSection requested as hydration chunk)
     - Client navigation: `/settings` → `/jobs` (DispatchForm not requested), `/settings` → `/agents` (DiagnosticsSection requested during navigation)
     - Interaction trigger: `/jobs` (DispatchForm requested when dialog opens)
   - Tenant navigation and route-data refresh
   - Settings live printer updates
   - Onboarding (inline render when tenants list is empty)
   - Layout freshness after mutations (create tenant, update role, verify layout tenant list updates)
   - Initial SSR shell state (view from URL path, tenant from `?tenant=` param, no hydration mismatch)
   - Tenant-scoped stale-state prevention (change tenant, verify live printers/jobs update to new tenant, no stale data from previous tenant)
   - RSC import-graph verification (manual code review confirming RSC files not imported into client component graph)
   - Fast 3G loading-skeleton timing (5 runs, median, document in `docs/roadmap.md`)

**Conditional checks** (non-gating, require external services, record "not run" with prerequisites when unavailable):
- Verify auth flow works (login, logout) with `APP_AUTH_PROVIDER=logto` (requires Logto endpoint and credentials)
- Verify device creation works (requires Hub API and printer)
- Verify job dispatch works (requires Hub API, printer, and 3MF file)
- Verify settings update works (requires Hub API and tenant admin role)

**Backward-compatibility checks** (non-gating, no external services required):
- Verify locale switching works (en/zh)
- Verify theme persistence (set theme, reload, verify theme persists)
- Verify language persistence (set language, reload, verify language persists)
- Verify sidebar-state persistence (toggle sidebar, reload, verify sidebar state persists)

**Validation**:
- All commands pass
- Manual checklist complete (mandatory + conditional with "not run" records)

## Dependencies Between Tasks

- Task 0 (bundle script, generate baseline before implementation) → Task 1 (server utilities) → Task 2 (shell provider/layout) → Task 3 (registrar/consumer) → Task 4 (RSC panels) → Task 5 (sidebar) → Task 6 (diagnostics extraction) → Task 7 (dispatch lazy load) → Task 8 (route group) → Task 9 (remove old paths/runtime) → Task 10 (revalidation) → Task 11 (tests, alongside implementation) → Task 12 (measure timing) → Task 13 (docs) → Task 14 (final verification)

**Note**: Task 11 tests are added alongside their corresponding implementation tasks (Task 1 tests with Task 1, Task 2 tests with Task 2, etc.), not as a separate phase.

## Rollback Strategy

- All changes committed as one atomic unit and squash-merged into main
- Rollback via single `git revert <squash-commit>` of the squash merge
- R1, R2, R3 share modified files and cannot be reverted independently without conflicts

## Notes

- No new npm packages required
- All changes are structural, not behavioral
- Existing 385 tests must pass
- New tests added for lazy loading, tenant navigation, stale cleanup, memoization, selected-tenant edge cases, registrar prop-change re-registration
- `dashboard-runtime.tsx` deleted (no backward compatibility wrapper, per no-legacy-fallback constraint)
- Old route pages deleted in Task 8 (atomic cutover)
- No fallback client-render paths in Task 9 (no legacy fallback)
