# Frontend Localization (中文 / English) — Design

Date: 2026-06-28
Scope: `frontend/` (Next.js 16 App Router, React 19, Tailwind v4). Backend out of scope.

## Goal

Add bilingual (Chinese `zh` and English `en`) localization to the Pandar frontend. Every user-visible string renders in the user's chosen language, on both server-rendered and client-rendered components, with no URL restructuring.

## Non-goals (YAGNI)

- No `[locale]` URL segment and no locale-aware URL redirects.
- No `middleware.ts`.
- No SEO/permalink handling beyond translating `<title>`/description via `generateMetadata`.
- No RTL support (both languages are LTR).
- No third locale plumbing; the supported set is exactly `['en', 'zh']`.
- Backend strings stay English; only frontend UI is localized.

## Decisions (from brainstorming)

1. **Locale strategy:** Accept-Language detection on first visit + persisted local state. No URL change.
2. **SSR signal:** The user's explicit choice is mirrored into a `locale` cookie so server components can read it. Cookie is authoritative for rendering; zustand (`pandar.settings`) mirrors it for client-side UI/persistence.
3. **i18n library:** `next-intl`, used in **non-segment mode** (no App Router `[locale]` segment).

## Architecture

### Locale resolution (no URL change)

- **Source of truth for SSR:** `locale` cookie (`en` | `zh`). Read by next-intl's `getRequestConfig` via `next/headers` `cookies()`.
- **First visit (no cookie):** `getRequestConfig` negotiates `Accept-Language` → picks `zh` if the header prefers any `zh*` variant, else `en`. Default `en`.
- **Client state:** zustand store under `pandar.settings.locale` mirrors the cookie (per `CLAUDE.md` convention). Lets the switcher UI reflect the choice instantly and persists across reloads. The client trusts the server-rendered locale (from `NextIntlClientProvider`) on mount, so SSR and client never disagree.
- **Switching:** the language switcher optimistically updates zustand, calls a server action to set the `locale` cookie, then `router.refresh()` so server components re-render in the new locale. No URL change, no full page reload.
- **No `middleware.ts`:** `getRequestConfig` performs cookie-or-Accept-Language negotiation on every request.
- **Existing `app/[locale]/sign-in` route is untouched.** It is a literal dynamic segment for Bambu Studio's `/en/sign-in` WebView entry, not a locale router. next-intl in non-segment mode does not touch routing, so there is no conflict. That page picks up translations from the cookie like every other page.

### next-intl mode

Non-segment mode: server components call `useTranslations()` (sync, works in RSC); client components consume translations via `NextIntlClientProvider` seeded by the server-resolved locale + messages. `getTranslations()` is used only inside `generateMetadata`. `setRequestLocale`/`generateStaticParams` are not used (segment-only APIs).

### File structure

```
frontend/
  i18n/
    request.ts          # getRequestConfig: cookie → Accept-Language → 'en'; load messages
    routing.ts          # locales = ['en','zh'], defaultLocale = 'en'
  messages/
    en.json             # nested namespaces (dashboard, admin, signIn, join, ...)
    zh.json
  app/
    layout.tsx          # getLocale(), <html lang={locale}>, NextIntlClientProvider wrapper, generateMetadata
    ...                 # components use useTranslations() / getTranslations()
  lib/
    settings-store.ts   # zustand persist('pandar.settings', { locale })
  components/
    language-switcher.tsx
    formatted-date.tsx   # wraps useFormatter().dateTime
```

## Translation coverage & namespaces

Full coverage: all ~260 user-visible English strings. Namespaces follow feature files so keys stay co-located with their components.

| Namespace | Source file(s) | Notes |
|---|---|---|
| `common` | dashboard-ui, confirm-dialog | EmptyState, buttons (Cancel/Continue), Metric labels |
| `header` | dashboard-header | "Pandar Operations", "Tenant", "View" |
| `nav` | dashboard-overview (`NAV_SECTIONS`) | Printers / Print jobs / Dispatch / Recovery / Diagnostics / Live activity / Admin |
| `overview` | dashboard-overview, dashboard-status | FleetStatusStrip, verdicts, StatCell ("online/offline/down/connected/active/failed") |
| `attention` | dashboard-attention, dashboard-overview | NeedsAttention, exception counts |
| `inventory` | dashboard-inventory | PrinterInventory, JobHistory |
| `dispatch` | dispatch-form | |
| `recovery` | recovery-actions, `formatJobRecoveryState` | recovery-state sentences, `formatDuration` |
| `diagnostics` | diagnostics-panel | |
| `runtime` | dashboard-runtime, dashboard-runtime-helpers | notifications, `ACTION_STATUS_MESSAGES`, `formatLiveState`, `formatAuthSource` |
| `admin` | admin-panel | users, tenant tokens, join links, audit events |
| `tenantSettings` | dashboard-runtime-sections | TenantSettings, RuntimeStatusPanel |
| `onboarding` | onboarding-panel | |
| `signIn` | plugin-sign-in, sign-in | |
| `join` | join | |
| `tokens` | dashboard-attention (`prettifyToken`/`statusMeta`) | known machine-status enums |

## Dynamic string builders — refactor plan

Each builder keeps its logic but returns translated strings via `t()`.

- `formatLiveState` → `t('runtime.liveState.{live|connecting|reconnecting|idle|unavailable}')`.
- `formatAuthSource` → `t('runtime.authSource.{requestCookie|appBearerToken|appApiToken|none}')`.
- `formatJobRecoveryState` → keyword-matching logic unchanged; returned sentences become `t('recovery.state.{printing|completed|failed|cancelled|waitingAgent|fileFailed|mqttFailed|queueFailed|waitingStart}')`.
- `formatDuration` → ICU plurals: `"{count, plural, one {1 minute} other {# minutes}}"`, plus a `lessThanMinute` literal.
- `formatPrinterMaterials` / `formatJobMaterial` / `formatArtifactMetadata` → split into discrete keys; ICU plural for tray counts.
- `ACTION_STATUS_MESSAGES` → `actionStatus.{refreshPartial|retryPartial}`; the unknown-status capitalization fallback in `formatActionStatus` stays.
- `prettifyToken` / `statusMeta` → these prettify **backend-derived** status enums (e.g. `external_spool` → "External spool"). Add a `tokens.*` namespace mapping the known statuses to translations; unknown tokens fall back to the existing prettify logic (we cannot translate arbitrary backend strings). Both `en` and `zh` get the mapping; `zh` values are the Chinese renderings of the known machine statuses.

## Plurals, interpolation, dates, numbers

- **ICU MessageFormat** (built into next-intl) for all plurals: exceptions, minutes/hours, AMS trays. Chinese plural rules handled automatically by ICU.
- **`{var}` interpolation** for status strings (`{name} is {status}`, `{online}/{total} online`).
- **Dates:** `formatDate` (dashboard-ui.tsx) currently hardcodes `toLocaleString('en-US', ...)`. Replace callers with a `<FormattedDate value={...} />` component using next-intl's `useFormatter().dateTime(date, { dateStyle: 'medium', timeStyle: 'short', timeZone: 'UTC' })`, so dates render in the active locale.
- **Bytes:** `formatBytes` localizes the numeric portion via `useFormatter().number(...)`; unit suffixes (B/KiB/MiB) are kept.

## Language switcher

- **Component:** one shared `<LanguageSwitcher />` rendered as a compact `EN | 中文` pair of toggle buttons (the active language is highlighted). Chosen over a `<select>` for its smaller footprint and immediate affordance at two characters per language.
- **Placement:** rendered in two places so every page is covered without global chrome:
  1. The dashboard `Header` (dashboard-header.tsx) — main app.
  2. The standalone page headers via `SectionHeader` (used by onboarding / sign-in / join) — so unauthenticated/onboarding users can switch too.
- **Switch flow:** optimistic `useSettings.setState({ locale: next })` → `setLocale(next)` server action → `router.refresh()`. No full page reload.

## Cookie wiring

Server action (`i18n/actions.ts` — kept separate from `app/actions.ts`, which holds dashboard-domain server actions):

```ts
'use server'
import { cookies } from 'next/headers'

export async function setLocale(locale: 'en' | 'zh') {
  cookies().set('locale', locale, {
    path: '/',
    maxAge: 60 * 60 * 24 * 365,
    sameSite: 'lax',
  })
}
```

## zustand store (`lib/settings-store.ts`)

```ts
import { create } from 'zustand'
import { persist } from 'zustand/middleware'

type Settings = { locale: 'en' | 'zh' }

export const useSettings = create<Settings>()(
  persist(() => ({ locale: 'en' }), { name: 'pandar.settings' }),
)
```

zustand persists the user's choice and drives the switcher's optimistic UI. The cookie remains authoritative for what the server renders.

## Locale negotiation (`i18n/request.ts`)

`getRequestConfig` reads the `locale` cookie via `cookies()`; if absent, negotiates `Accept-Language` (any `zh*` → `zh`, else `en`); default `en`. Validates against `['en', 'zh']`; invalid values fall back to `en`. Loads `../messages/{locale}.json`.

## Layout metadata

`app/layout.tsx`'s static `metadata` (title "Pandar", description) is replaced by `generateMetadata` + `getTranslations()` so `<title>` and description translate. `<html lang>` becomes the resolved locale.

## Verification

The repository has no frontend unit-test framework; `frontend/package.json` exposes only `lint`, `build`, `dev`, `start`.

- `npm run lint` passes (project's lint script).
- `npm run build` succeeds — catches RSC/client boundary + TypeScript errors introduced by next-intl wiring.
- Manual verification:
  - Switch language from the dashboard `Header`; hard-reload; confirm the choice persists (cookie + zustand) and server components re-render in the new locale.
  - Switch language from a standalone page header (onboarding/sign-in/join); confirm those server-rendered pages translate.
  - Simulate first-visit Accept-Language negotiation: `curl -H 'Accept-Language: zh' <url>` with no `locale` cookie; confirm the response is Chinese.
- Update `docs/roadmap.md` per repo convention ("每次更新完代码都更新路线图").

## Risks & notes

- **next-intl non-segment mode** is less commonly documented than the segment pattern; the implementation must avoid `setRequestLocale`/`generateStaticParams` (segment-only APIs) and rely solely on `getRequestConfig` for locale resolution.
- **`useFormatter` is a hook**, so sites that currently call the plain `formatDate`/`formatBytes` functions inside non-component code paths must be converted to component-based formatting (`<FormattedDate />`) or receive a formatter via props. The refactor plan accounts for this.
- **Backend-derived tokens** (`prettifyToken`) cannot all be translated; the design translates the known set and falls back to prettification for unknown values. This is an accepted limitation, not a bug.
