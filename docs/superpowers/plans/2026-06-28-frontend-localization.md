# Frontend Localization (中文 / English) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add bilingual (Chinese `zh` and English `en`) localization to the Pandar frontend using `next-intl` in non-segment (cookie-based) mode, translating every user-visible string on both server-rendered and client-rendered components with no URL restructuring.

**Architecture:** A `locale` cookie (authoritative for SSR) drives `next-intl`'s `getRequestConfig`; it negotiates `Accept-Language` on first visit. `NextIntlClientProvider` in the root layout seeds client components. A zustand store under `pandar.settings` mirrors the cookie for the switcher's optimistic UI. Dynamic string-builder helpers are refactored to accept a `t` translator argument (and a `formatDate` bound function where they embed dates). A locale-independent `reason` field is added to `AttentionItem` so action dispatch no longer depends on translated title text.

**Tech Stack:** Next.js 16 App Router, React 19, TypeScript, Tailwind v4, `next-intl`, `zustand` (persist).

## Global Constraints

- **Supported locales:** exactly `['en', 'zh']`. Default locale `en`.
- **No URL change.** No `[locale]` App Router segment for i18n. No `middleware.ts`. Do NOT use `setRequestLocale`/`generateStaticParams` (segment-only APIs).
- **Existing `app/[locale]/sign-in/page.tsx` MUST stay untouched** (Bambu Studio `/en/sign-in` WebView alias). It just re-exports `plugin-sign-in/page`.
- **Do not introduce a frontend test framework.** The repo has none (`frontend/package.json` exposes only `lint`, `build`, `dev`, `start`). Verification per task = `npm run lint` + `npm run build` + manual smoke. Per `AGENTS.md`: avoid over-engineering.
- **No comments** added to code unless logic is non-self-evident (per `AGENTS.md`).
- **Run `npm run lint` and `npm run build` from `frontend/`.** Build = `tsc` type-check via Next; catches RSC/client boundary + type errors.
- **Preserve error context:** do not swallow the existing English fallback for backend-derived status tokens (`prettifyToken`).
- **Commit policy:** only commit when the user explicitly asks (`AGENTS.md`). Task steps show `git add`/`git commit` as the _recommended_ commit points but execution must ask first.
- All file paths are relative to `frontend/` unless noted.

## Spec Reference

`docs/superpowers/specs/2026-06-28-frontend-localization-design.md`

## File Structure (created/modified)

**New files:**

- `i18n/routing.ts` — locale constants.
- `i18n/request.ts` — `getRequestConfig`: cookie → Accept-Language → `en`; loads messages.
- `i18n/actions.ts` — `'use server'` `setLocale(locale)` action.
- `messages/en.json`, `messages/zh.json` — namespaced translation dictionaries.
- `lib/settings-store.ts` — zustand `pandar.settings` store (locale field).
- `components/language-switcher.tsx` — shared toggle buttons.
- `components/formatted-date.tsx` — `<FormattedDate value={...} />` using `useFormatter().dateTime`.

**Modified files (server):**

- `app/layout.tsx` — wrap in `NextIntlClientProvider`, dynamic `<html lang>`, `generateMetadata`.

**Modified files (helpers, signature changes — task 4):**

- `app/dashboard-runtime-helpers.ts` — `formatLiveState`, `formatAuthSource`, `formatJobRecoveryState`, `formatDuration`, `formatPrinterMaterials`, `formatJobMaterial`, `formatArtifactMetadata` gain `t` (+ date) params.
- `app/dashboard-attention.ts` — `AttentionItem` gains `reason`; `statusMeta`/`prettifyToken` gain `tokens` translation map + fallback.
- `app/dashboard-status.tsx` — `computeVerdict` gains `t`; `AttentionAction` switches on `item.reason`.
- `app/dashboard-ui.tsx` — `formatDate` deprecated in favor of `<FormattedDate>`; `formatBytes` accepts optional number formatter.
- `app/job-format.ts` — `formatLayers`/`formatRemaining` gain `t`.

**Modified files (component string replacement):** `dashboard-header.tsx`, `dashboard-overview.tsx`, `dashboard-inventory.tsx`, `dispatch-form.tsx`, `recovery-actions.tsx`, `diagnostics-panel.tsx`, `dashboard-runtime-sections.tsx`, `dashboard-runtime.tsx`, `admin-panel.tsx`, `onboarding-panel.tsx`, `plugin-sign-in/page.tsx`, `plugin-sign-in/plugin-ticket-form.tsx`, `join/page.tsx`, `join/token-form.tsx`.

---

## Translation conventions (apply to every translation task)

1. **Import the hook:** `import { useTranslations } from 'next-intl'`. At the top of a component body: `const t = useTranslations('<namespace>')`.
2. **Replace each English string literal** that is user-visible with `t('<key>')`. For interpolation use ICU: `t('greeting', { name })` → `"greeting": "Hello {name}"`. For plurals: `t('exceptions', { count })` → `"{count, plural, one {# exception} other {# exceptions}}"`.
3. **Helper functions** that returned English now receive the translator: pass `t` (and a bound `formatDate` where the helper embeds a date) from the calling component.
4. **Do NOT translate:** `aria-hidden` strings, `name=`/`value=`/`type=` attributes, CSS classes, route paths, HTTP-error codes (`http_${status}`), `font-mono` IDs/slugs, or backend values that are already locale-neutral.
5. **Keep JSX structure, Tailwind classes, and component props identical** — only swap the string expression.
6. **Both locales must contain the same key set.** After editing a namespace, update BOTH `messages/en.json` and `messages/zh.json`.
7. **Verify after every task:** run `cd frontend && npm run lint && npm run build`.

Representative before/after (apply this pattern mechanically to every literal):

```tsx
// BEFORE
export function Header({ apiUrl, tenants, selectedTenant }: Props) {
  return (
    <header ...>
      <h1 className="text-2xl font-semibold">Pandar Operations</h1>
      <p className="mt-1 text-sm text-slate-600">Tenant printer inventory from {apiUrl}</p>
      ...
      <button ... type="submit">View</button>
    </header>
  )
}

// AFTER
import { useTranslations } from 'next-intl'

export function Header({ apiUrl, tenants, selectedTenant }: Props) {
  const t = useTranslations('header')
  return (
    <header ...>
      <h1 className="text-2xl font-semibold">{t('title')}</h1>
      <p className="mt-1 text-sm text-slate-600">{t('inventoryFrom', { apiUrl })}</p>
      ...
      <button ... type="submit">{t('view')}</button>
    </header>
  )
}
```

with `messages/en.json`:

```json
{
  "header": {
    "title": "Pandar Operations",
    "inventoryFrom": "Tenant printer inventory from {apiUrl}",
    "view": "View"
  }
}
```

and `messages/zh.json`:

```json
{
  "header": {
    "title": "Pandar 运维控制台",
    "inventoryFrom": "来自 {apiUrl} 的租户打印机清单",
    "view": "查看"
  }
}
```

---

## Task 1: Install next-intl and wire request config + root provider

**Files:**

- Create: `frontend/i18n/routing.ts`
- Create: `frontend/i18n/request.ts`
- Create: `frontend/messages/en.json`
- Create: `frontend/messages/zh.json`
- Modify: `frontend/app/layout.tsx`
- Modify: `frontend/package.json` (via `npm install`)

**Interfaces:**

- Produces: `i18n/routing.ts` exports `locales` (`readonly ['en','zh']`), `defaultLocale` (`'en'`), `Locale` type, `isLocale(value)`. `i18n/request.ts` default-exports the `getRequestConfig` callable expected by `next-intl`. `app/layout.tsx` renders `<NextIntlClientProvider locale={locale} messages={messages}>`.

- [ ] **Step 1: Install next-intl**

Run:

```bash
cd frontend && npm install next-intl@^3
```

Expected: `next-intl` added to `dependencies` in `package.json`; `package-lock.json` updated.

- [ ] **Step 2: Create `i18n/routing.ts`**

```ts
export const locales = ["en", "zh"] as const;
export type Locale = (typeof locales)[number];
export const defaultLocale: Locale = "en";

export function isLocale(value: string | undefined | null): value is Locale {
  return value === "en" || value === "zh";
}
```

- [ ] **Step 3: Create `i18n/request.ts`**

next-intl in non-segment mode: resolve the locale from cookie → `Accept-Language` → default, then load the messages JSON.

```ts
import { getRequestConfig } from "next-intl/server";
import { headers, cookies } from "next/headers";

import { defaultLocale, isLocale, type Locale } from "./routing";

export default getRequestConfig(async () => {
  const cookieStore = await cookies();
  const headerList = await headers();
  const cookieLocale = cookieStore.get("locale")?.value;
  const acceptLanguage = headerList.get("accept-language") ?? "";
  const locale: Locale = resolveLocale(cookieLocale, acceptLanguage);
  return {
    locale,
    messages: (await import(`../messages/${locale}.json`)).default,
  };
});

function resolveLocale(
  cookie: string | undefined,
  acceptLanguage: string,
): Locale {
  if (isLocale(cookie)) {
    return cookie;
  }
  if (/\bzh\b|zh-/i.test(acceptLanguage)) {
    return "zh";
  }
  return defaultLocale;
}
```

- [ ] **Step 4: Seed `messages/en.json` and `messages/zh.json`**

Only the metadata keys are needed for this task; later tasks merge their namespaces. Use this exact content:

`messages/en.json`:

```json
{
  "meta": {
    "title": "Pandar",
    "description": "Bambu Studio cloud alternative"
  }
}
```

`messages/zh.json`:

```json
{
  "meta": {
    "title": "Pandar",
    "description": "Bambu Studio 的云端替代方案"
  }
}
```

- [ ] **Step 5: Rewrite `app/layout.tsx`**

Replace the whole file. It must read the locale + messages on the server, set `<html lang>`, wrap children in the provider, and use `generateMetadata` for translated title/description.

```tsx
import type { Metadata } from "next";
import { Inter } from "next/font/google";
import { NextIntlClientProvider } from "next-intl";
import { getLocale, getTranslations } from "next-intl/server";
import type { ReactNode } from "react";

import "./globals.css";

const inter = Inter({
  subsets: ["latin"],
  variable: "--font-inter",
  display: "swap",
});

export async function generateMetadata(): Promise<Metadata> {
  const t = await getTranslations("meta");
  return { title: t("title"), description: t("description") };
}

export default async function RootLayout({
  children,
}: Readonly<{ children: ReactNode }>) {
  const locale = await getLocale();
  return (
    <html className={inter.variable} lang={locale}>
      <body>
        <NextIntlClientProvider locale={locale}>
          {children}
        </NextIntlClientProvider>
      </body>
    </html>
  );
}
```

> Note: passing only `locale` to `NextIntlClientProvider` (not `messages`) makes it inherit messages from the request config automatically (next-intl v3 behavior). If the build errors requesting explicit `messages`, pass `messages={(await getTranslations('meta')) && (await import messages)}` pattern — but first try the inherit form above.

- [ ] **Step 6: Build to verify wiring**

Run:

```bash
cd frontend && npm run lint && npm run build
```

Expected: both succeed. `npm run build` confirms `i18n/request.ts` is picked up by Next's `i18n` directory convention. No runtime check yet (no rendered translations).

- [ ] **Step 7: Manual smoke**

Run `cd frontend && npm run dev`. Open `/`. Confirm the page still renders. View page source: `<html lang="en">`. Then `curl -H 'Accept-Language: zh' http://localhost:3000/ | head -5` — confirm `<html lang="zh">` (accept-Language negotiation). Set cookie `locale=zh` in browser, reload, confirm `<html lang="zh">`.

- [ ] **Step 8: Commit (if user permits)**

```bash
git add frontend/package.json frontend/package-lock.json frontend/i18n frontend/messages frontend/app/layout.tsx
git commit -m "feat(frontend): wire next-intl locale resolution and root provider"
```

---

## Task 2: zustand settings store + setLocale action + LanguageSwitcher

**Files:**

- Create: `frontend/lib/settings-store.ts`
- Create: `frontend/i18n/actions.ts`
- Create: `frontend/components/language-switcher.tsx`

**Interfaces:**

- Produces: `useSettings` zustand store with `{ locale: 'en' | 'zh' }` persisted under key `pandar.settings`, and `setLocale(locale: Locale): Promise<void>` server action. `<LanguageSwitcher />` is a client component rendering two toggle buttons.

- [ ] **Step 1: Create `lib/settings-store.ts`**

```ts
import { create } from "zustand";
import { persist } from "zustand/middleware";

import { defaultLocale, type Locale } from "../i18n/routing";

type Settings = {
  locale: Locale;
};

export const useSettings = create<Settings>()(
  persist(() => ({ locale: defaultLocale }), { name: "pandar.settings" }),
);
```

- [ ] **Step 2: Create `i18n/actions.ts`**

```ts
"use server";

import { cookies } from "next/headers";

import { isLocale, type Locale } from "./routing";

export async function setLocale(locale: Locale): Promise<void> {
  if (!isLocale(locale)) {
    return;
  }
  const cookieStore = await cookies();
  cookieStore.set("locale", locale, {
    path: "/",
    maxAge: 60 * 60 * 24 * 365,
    sameSite: "lax",
  });
}
```

- [ ] **Step 3: Create `components/language-switcher.tsx`**

```tsx
"use client";

import { useLocale } from "next-intl";
import { useRouter } from "next/navigation";
import { useTransition } from "react";

import { setLocale } from "../i18n/actions";
import { locales, type Locale } from "../i18n/routing";
import { useSettings } from "../lib/settings-store";

const LABELS: Record<Locale, string> = {
  en: "EN",
  zh: "中文",
};

export function LanguageSwitcher() {
  const active = useLocale() as Locale;
  const router = useRouter();
  const [pending, startTransition] = useTransition();
  const setSettings = useSettings((state) => state.locale);

  const choose = (next: Locale) => {
    if (next === active || pending) {
      return;
    }
    startTransition(async () => {
      useSettings.setState({ locale: next });
      await setLocale(next);
      router.refresh();
    });
  };

  void setSettings;

  return (
    <div className="inline-flex items-center gap-1 rounded-md border border-slate-300 bg-white p-0.5">
      {locales.map((locale) => {
        const isActive = locale === active;
        return (
          <button
            key={locale}
            className={`rounded px-2 py-0.5 text-xs font-medium transition-colors ${
              isActive
                ? "bg-slate-900 text-white"
                : "text-slate-600 hover:bg-slate-100"
            }`}
            disabled={pending}
            onClick={() => choose(locale)}
            type="button"
          >
            {LABELS[locale]}
          </button>
        );
      })}
    </div>
  );
}
```

- [ ] **Step 4: Build to verify**

Run:

```bash
cd frontend && npm run lint && npm run build
```

Expected: succeeds. (The switcher is not yet rendered anywhere; placement happens in Task 5.)

- [ ] **Step 5: Commit (if user permits)**

```bash
git add frontend/lib frontend/i18n/actions.ts frontend/components/language-switcher.tsx
git commit -m "feat(frontend): add settings store, setLocale action, and language switcher"
```

---

## Task 3: FormattedDate component + formatBytes localization

**Files:**

- Create: `frontend/components/formatted-date.tsx`
- Modify: `frontend/app/dashboard-ui.tsx`

**Interfaces:**

- Produces: `<FormattedDate value={string} />` rendering a locale-aware date (`dateStyle: 'medium'`, `timeStyle: 'short'`, `timeZone: 'UTC'`). `formatBytes(value, formatNumber?)` keeps existing behavior when no formatter is passed (so non-component callers still compile) but accepts an optional `formatNumber: (n: number) => string` to localize the numeric portion.
- `formatDate` in `dashboard-ui.tsx` is kept exported for now (still referenced by helpers until Task 4) but its callers migrate to `<FormattedDate>` where the call site is JSX.

- [ ] **Step 1: Create `components/formatted-date.tsx`**

```tsx
"use client";

import { useFormatter } from "next-intl";

const parseable = (value: string) => {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date;
};

export function FormattedDate({ value }: { value: string }) {
  const date = parseable(value);
  const format = useFormatter();
  if (!date) {
    return <>{value}</>;
  }
  return (
    <>
      {format.dateTime(date, {
        dateStyle: "medium",
        timeStyle: "short",
        timeZone: "UTC",
      })}
    </>
  );
}
```

- [ ] **Step 2: Update `formatBytes` in `app/dashboard-ui.tsx`**

Find:

```ts
export function formatBytes(value: number) {
  if (value < 1024) {
    return `${value} B`;
  }
  if (value < 1024 * 1024) {
    return `${(value / 1024).toFixed(1)} KiB`;
  }

  return `${(value / (1024 * 1024)).toFixed(1)} MiB`;
}
```

Replace with:

```ts
export function formatBytes(
  value: number,
  formatNumber?: (n: number) => string,
) {
  const fmt = (n: number) => (formatNumber ? formatNumber(n) : n.toFixed(1));
  if (value < 1024) {
    return `${formatNumber ? formatNumber(value) : value} B`;
  }
  if (value < 1024 * 1024) {
    return `${fmt(value / 1024)} KiB`;
  }

  return `${fmt(value / (1024 * 1024))} MiB`;
}
```

- [ ] **Step 3: Build to verify**

Run: `cd frontend && npm run lint && npm run build`
Expected: succeeds. (No JSX callers changed yet; `FormattedDate` is ready for later tasks. `formatDate` still used by helpers — handled in Task 4.)

- [ ] **Step 4: Commit (if user permits)**

```bash
git add frontend/components/formatted-date.tsx frontend/app/dashboard-ui.tsx
git commit -m "feat(frontend): add FormattedDate component and locale-ready formatBytes"
```

---

## Task 4: Refactor dynamic string builders to be locale-aware

This is the linchpin task. It changes helper signatures so they accept a translator `t` (and a bound date formatter where they embed dates), and adds a locale-independent `reason` field to `AttentionItem` so action dispatch no longer keys off translated title text. **No new user-facing strings are rendered yet** (callers still pass English values until later tasks) — but the helpers become ready. The `runtime`, `recovery`, `tokens`, `attention`, and `overview` message namespaces used by these helpers are created here so later tasks can rely on them.

**Files:**

- Modify: `frontend/app/dashboard-runtime-helpers.ts`
- Modify: `frontend/app/dashboard-attention.ts`
- Modify: `frontend/app/dashboard-status.tsx`
- Modify: `frontend/app/job-format.ts`
- Modify: `frontend/app/dashboard-ui.tsx`
- Modify: `frontend/messages/en.json`
- Modify: `frontend/messages/zh.json`

**Interfaces:**

- New types: `AttentionReason` = `'agent_unhealthy' | 'printer_offline' | 'job_print_failed' | 'job_dispatch_failed' | 'job_stalled'`. `AttentionItem` gains `reason: AttentionReason`; its `title`/`label` become **derived at render time** (helpers stop storing English).
- Helper signatures (final form):
  - `formatLiveState(state, t)` → `string`
  - `formatAuthSource(source, t)` → `string`
  - `formatJobRecoveryState(job, t)` → `string`
  - `formatDuration(ms, t)` → `string`
  - `formatPrinterMaterials(printer, t, formatDate)` → `{ summary, detail }`
  - `formatJobMaterial(job, t)` → `string`
  - `formatArtifactMetadata(job, t, formatDate)` → `string`
  - `computeVerdict(args, t)` → `Verdict` (with translated `title`/`detail`)
  - `formatLayers(job, t)` / `formatRemaining(minutes, t)` → `string`
  - `statusMeta(value, t)` / `prettifyToken(value, t)` → use `tokens.*` namespace, fallback to prettify.
- These helpers take a next-intl translator scoped to the right namespace. `t` is typed as the next-intl translator; since it's structural, type as a thin callable: `type Translator = (key: string, values?: Record<string, string | number>) => string`. Callers pass their `useTranslations('ns')` directly (structurally compatible).

- [ ] **Step 1: Add the `runtime`, `recovery`, `tokens`, `attention`, `overview.verdict` namespaces to messages**

Merge these namespaces into the existing JSON objects (keep `meta` from Task 1). Final shape after this step:

`messages/en.json`:

```json
{
  "meta": {
    "title": "Pandar",
    "description": "Bambu Studio cloud alternative"
  },
  "runtime": {
    "liveState": {
      "live": "Connected",
      "connecting": "Connecting",
      "disconnected": "Reconnecting",
      "idle": "Idle",
      "unavailable": "Unavailable",
      "error": "Unavailable"
    },
    "authSource": {
      "request_cookie": "Request cookie",
      "app_auth_bearer_token": "App bearer token",
      "app_api_token": "App API token",
      "none": "No auth"
    },
    "actionStatus": {
      "refresh_partial": "Some refreshes could not be queued — review the list",
      "retry_partial": "Some retries could not be queued — review the list"
    },
    "notification": {
      "liveTitle": "Live connection",
      "liveUnavailable": "Live updates unavailable because no server-side auth token is configured.",
      "liveRetryingUnavailable": "Live updates unavailable; retrying.",
      "liveDisconnectedRetrying": "Live updates disconnected; retrying.",
      "printerStateTitle": "Printer state",
      "printFailedTitle": "Print failed",
      "printCompleteTitle": "Print complete",
      "errorsIncomplete": "Hub data is incomplete."
    }
  },
  "recovery": {
    "state": {
      "printing": "Printing now",
      "completed": "Print completed",
      "failed": "Print failed",
      "cancelled": "Print cancelled",
      "waitingAgent": "Waiting for the agent to come back online",
      "fileFailed": "Could not send the file to the printer",
      "mqttFailed": "Printer did not accept the start command",
      "queueFailed": "Could not queue the job at the hub",
      "waitingStart": "Waiting for the print to start"
    },
    "duration": {
      "lessThanMinute": "less than a minute",
      "minutes": "{count, plural, =1 {1 minute} other {# minutes}}",
      "hours": "{count, plural, =1 {1 hour} other {# hours}}"
    }
  },
  "tokens": {
    "online": "Online",
    "offline": "Offline",
    "problem": "Problem",
    "connecting": "Connecting",
    "running": "Running",
    "printing": "Printing",
    "ready": "Ready",
    "queued": "Queued",
    "sent": "Sent",
    "acknowledged": "Acknowledged",
    "pending": "Pending",
    "succeeded": "Succeeded",
    "completed": "Completed",
    "failed": "Failed",
    "cancelled": "Cancelled",
    "unavailable": "Unavailable",
    "error": "Error",
    "down": "Down",
    "warning": "Warning",
    "degraded": "Degraded",
    "ok": "OK"
  },
  "attention": {
    "agent": { "title": "Agent {status}", "label": "{name} is {status}" },
    "printer": { "title": "Printer {status}", "label": "{name} is {status}" },
    "jobPrintFailed": { "title": "Print failed" },
    "jobDispatchFailed": { "title": "Dispatch failed" },
    "jobStalled": {
      "title": "Job stalled",
      "label": "{filename} · no progress for {duration}"
    },
    "unknownAgent": "Unknown agent"
  },
  "overview": {
    "verdict": {
      "noFleet": {
        "title": "No fleet configured",
        "detail": "Connect an agent to start monitoring your printers."
      },
      "liveUnavailable": {
        "title": "Live updates unavailable",
        "detail": "Reconnecting — showing the last known state."
      },
      "liveDisconnected": {
        "title": "Live updates disconnected",
        "detail": "Reconnecting — showing the last known state."
      },
      "nominal": {
        "title": "All systems nominal",
        "detail": "No exceptions across the fleet."
      },
      "needAttention": {
        "title": "{count, plural, =1 {# item needs attention} other {# items need attention}}",
        "detailCritical": "Failures detected — review below.",
        "detailOther": "Review the items below."
      }
    }
  }
}
```

`messages/zh.json`:

```json
{
  "meta": { "title": "Pandar", "description": "Bambu Studio 的云端替代方案" },
  "runtime": {
    "liveState": {
      "live": "已连接",
      "connecting": "连接中",
      "disconnected": "重新连接中",
      "idle": "空闲",
      "unavailable": "不可用",
      "error": "不可用"
    },
    "authSource": {
      "request_cookie": "请求 Cookie",
      "app_auth_bearer_token": "应用 Bearer 令牌",
      "app_api_token": "应用 API 令牌",
      "none": "无身份认证"
    },
    "actionStatus": {
      "refresh_partial": "部分刷新未能入队——请检查列表",
      "retry_partial": "部分重试未能入队——请检查列表"
    },
    "notification": {
      "liveTitle": "实时连接",
      "liveUnavailable": "由于未配置服务端认证令牌，实时更新不可用。",
      "liveRetryingUnavailable": "实时更新不可用，正在重试。",
      "liveDisconnectedRetrying": "实时更新已断开，正在重试。",
      "printerStateTitle": "打印机状态",
      "printFailedTitle": "打印失败",
      "printCompleteTitle": "打印完成",
      "errorsIncomplete": "Hub 数据不完整。"
    }
  },
  "recovery": {
    "state": {
      "printing": "正在打印",
      "completed": "打印已完成",
      "failed": "打印失败",
      "cancelled": "打印已取消",
      "waitingAgent": "等待 Agent 重新上线",
      "fileFailed": "无法将文件发送到打印机",
      "mqttFailed": "打印机未接受启动指令",
      "queueFailed": "无法在 Hub 排队该任务",
      "waitingStart": "等待开始打印"
    },
    "duration": {
      "lessThanMinute": "不到一分钟",
      "minutes": "{count, plural, other {# 分钟}}",
      "hours": "{count, plural, other {# 小时}}"
    }
  },
  "tokens": {
    "online": "在线",
    "offline": "离线",
    "problem": "异常",
    "connecting": "连接中",
    "running": "运行中",
    "printing": "打印中",
    "ready": "就绪",
    "queued": "已排队",
    "sent": "已发送",
    "acknowledged": "已确认",
    "pending": "待处理",
    "succeeded": "成功",
    "completed": "已完成",
    "failed": "失败",
    "cancelled": "已取消",
    "unavailable": "不可用",
    "error": "错误",
    "down": "离线",
    "warning": "警告",
    "degraded": "降级",
    "ok": "正常"
  },
  "attention": {
    "agent": { "title": "Agent {status}", "label": "{name} 处于 {status}" },
    "printer": { "title": "打印机 {status}", "label": "{name} 处于 {status}" },
    "jobPrintFailed": { "title": "打印失败" },
    "jobDispatchFailed": { "title": "派发失败" },
    "jobStalled": {
      "title": "任务停滞",
      "label": "{filename} · {duration} 无进展"
    },
    "unknownAgent": "未知 Agent"
  },
  "overview": {
    "verdict": {
      "noFleet": {
        "title": "尚未配置机队",
        "detail": "连接一个 Agent 以开始监控打印机。"
      },
      "liveUnavailable": {
        "title": "实时更新不可用",
        "detail": "正在重新连接——显示最近一次已知状态。"
      },
      "liveDisconnected": {
        "title": "实时更新已断开",
        "detail": "正在重新连接——显示最近一次已知状态。"
      },
      "nominal": { "title": "一切正常", "detail": "机队没有异常。" },
      "needAttention": {
        "title": "{count, plural, other {# 项需要关注}}",
        "detailCritical": "检测到故障——请查看下方。",
        "detailOther": "请查看下方项目。"
      }
    }
  }
}
```

- [ ] **Step 2: Add a shared `Translator` type and refactor `dashboard-runtime-helpers.ts`**

At the top of `frontend/app/dashboard-runtime-helpers.ts`, add after existing imports:

```ts
import type { Job, Printer } from "./dashboard-types";
```

Add a type alias and refactor each function. The complete refactored helper section (replace `formatLiveState`, `formatAuthSource`, `formatJobRecoveryState`, `formatDuration`, `formatPrinterMaterials`, `formatJobMaterial`, `formatArtifactMetadata`):

```ts
export type Translator = (
  key: string,
  values?: Record<string, string | number>,
) => string;
type DateFmt = (value: string) => string;

export function formatLiveState(state: LiveState, t: Translator): string {
  switch (state) {
    case "live":
      return t("live");
    case "connecting":
      return t("connecting");
    case "disconnected":
      return t("disconnected");
    case "idle":
      return t("idle");
    case "unavailable":
    case "error":
      return t("unavailable");
  }
}

export function formatAuthSource(
  source: AuthMetadata["source"],
  t: Translator,
): string {
  return t(source);
}

export function formatJobRecoveryState(job: Job, t: Translator): string {
  const dispatch = job.status.toLowerCase();
  const command = job.command.status.toLowerCase();
  const physical = job.print.status.toLowerCase();
  const message = `${job.error ?? ""} ${job.print.error ?? ""}`.toLowerCase();

  if (physical === "running") return t("printing");
  if (physical === "completed") return t("completed");
  if (physical === "failed") return t("failed");
  if (physical === "cancelled") return t("cancelled");
  if (dispatch === "queued" || command === "queued") return t("waitingAgent");
  if (
    message.includes("upload") ||
    message.includes("transfer") ||
    message.includes("sftp") ||
    message.includes("file")
  ) {
    return t("fileFailed");
  }
  if (message.includes("mqtt") || message.includes("publish")) {
    return t("mqttFailed");
  }
  if (dispatch === "failed" || command === "failed") return t("queueFailed");
  return t("waitingStart");
}

export function formatDuration(ms: number, t: Translator): string {
  const minutes = Math.round(ms / 60000);
  if (minutes < 1) return t("lessThanMinute");
  if (minutes < 60) return t("minutes", { count: minutes });
  const hours = Math.round(minutes / 60);
  return t("hours", { count: hours });
}

export function formatPrinterMaterials(
  printer: Printer,
  t: Translator,
  formatDate: DateFmt,
) {
  const materials = printer.materials;
  if (!materials) {
    return { summary: t("noMaterial"), detail: t("awaitingReport") };
  }
  const amsTrays = materials.ams_units.reduce(
    (count, unit) =>
      count + (unit.trays?.filter((tray) => tray.exists !== false).length ?? 0),
    0,
  );
  const external = materials.external_spools.filter(
    (spool) => spool.exists !== false,
  ).length;
  const active = materials.active_tray
    ? materials.active_tray.kind === "external"
      ? t("externalSpool")
      : t("amsSlot", {
          ams: materials.active_tray.ams_id ?? "-",
          tray: materials.active_tray.tray_id ?? "-",
        })
    : t("noActiveTray");
  return {
    summary: t("amsSummary", { trays: amsTrays, external }),
    detail: t("activeDetail", {
      active,
      observed: formatDate(materials.observed_at),
    }),
  };
}

export function formatJobMaterial(job: Job, t: Translator): string {
  const usage = job.material.filament_usage;
  if (usage.length > 0) {
    return usage
      .map((row) => {
        const slot =
          row.external_id !== null
            ? t("externalSlot", { tray: row.tray_id ?? "-" })
            : t("amsSlot", {
                ams: row.ams_id ?? "-",
                tray: row.tray_id ?? "-",
              });
        return t("usageRow", {
          index: row.slot_index,
          slot,
          type: row.filament_type ?? row.filament_id ?? "",
        }).trim();
      })
      .join(", ");
  }
  const mappings = [
    job.material.ams_mapping
      ? t("amsMapping", { count: job.material.ams_mapping.length })
      : null,
    job.material.ams_mapping2
      ? t("amsMapping2", { count: job.material.ams_mapping2.length })
      : null,
  ].filter(Boolean);
  return mappings.length > 0 ? mappings.join(", ") : t("noMapping");
}

export function formatArtifactMetadata(
  job: Job,
  t: Translator,
  formatDate: DateFmt,
): string {
  const metadata = job.artifact.metadata;
  if (!metadata) {
    return t("noMetadata");
  }

  const plate =
    metadata.plates.find(
      (candidate) => candidate.plate_id === metadata.default_plate_id,
    ) ?? metadata.plates[0];
  const plateLabel = metadata.default_plate_id
    ? t("plate", { id: metadata.default_plate_id })
    : t("plateNone");
  const objects = plate?.objects.length
    ? plate.objects.join(", ")
    : t("noObjects");
  const filament =
    plate?.filaments
      .map((row) => row.filament_type ?? row.filament_id)
      .filter(Boolean)
      .join(", ") || t("noFilament");

  return t("artifactSummary", {
    name: metadata.display_name,
    plate: plateLabel,
    objects,
    filament,
  });
}
```

The new `material` / `metadata` sub-keys these helpers reference go under a fresh `material` namespace. Add to `messages/en.json` `runtime`-adjacent top-level (merge into existing JSON, keep prior keys):

`messages/en.json` add:

```json
"material": {
  "noMaterial": "No material state",
  "awaitingReport": "Awaiting printer report",
  "externalSpool": "External spool",
  "amsSlot": "AMS {ams}:{tray}",
  "noActiveTray": "No active tray",
  "amsSummary": "{trays, plural, =1 {1 AMS tray} other {# AMS trays}}, {external} external",
  "activeDetail": "{active} · {observed}",
  "externalSlot": "external {tray}",
  "usageRow": "{index}: {slot} {type}",
  "amsMapping": "ams_mapping {count}",
  "amsMapping2": "ams_mapping2 {count}",
  "noMapping": "No material mapping",
  "noMetadata": "No slicer metadata",
  "plate": "plate {id}",
  "plateNone": "plate -",
  "noObjects": "no objects",
  "noFilament": "no filament",
  "artifactSummary": "{name} · {plate} · {objects} · {filament}"
}
```

`messages/zh.json` add:

```json
"material": {
  "noMaterial": "无耗材状态",
  "awaitingReport": "等待打印机上报",
  "externalSpool": "外部料盘",
  "amsSlot": "AMS {ams}:{tray}",
  "noActiveTray": "无激活料槽",
  "amsSummary": "{trays, plural, other {# 个 AMS 料槽}}，{external} 个外部",
  "activeDetail": "{active} · {observed}",
  "externalSlot": "外部 {tray}",
  "usageRow": "{index}：{slot} {type}",
  "amsMapping": "ams_mapping {count}",
  "amsMapping2": "ams_mapping2 {count}",
  "noMapping": "无耗材映射",
  "noMetadata": "无切片元数据",
  "plate": "盘子 {id}",
  "plateNone": "盘子 -",
  "noObjects": "无对象",
  "noFilament": "无耗材",
  "artifactSummary": "{name} · {plate} · {objects} · {filament}"
}
```

> Callers must scope `t` to the matching namespace: `formatLiveState`/`formatAuthSource` use `runtime.liveState` / `runtime.authSource` (so pass `useTranslations('runtime.liveState')` etc. — or pass a parent and key accordingly). To keep call sites simple, **scope the translator to the helper's own namespace** and have callers do `const tLive = useTranslations('runtime.liveState')`. The exact scoping each caller uses is specified in the consuming task. The keys above are the leaf keys under that namespace.

- [ ] **Step 3: Add `reason` to `AttentionItem` and refactor `dashboard-attention.ts`**

In `frontend/app/dashboard-attention.ts`:

Add a type and field. Replace the `AttentionItem` type:

```ts
export type AttentionReason =
  | "agent_unhealthy"
  | "printer_offline"
  | "job_print_failed"
  | "job_dispatch_failed"
  | "job_stalled";

export type AttentionItem = {
  id: string;
  agentId: string;
  agentName: string;
  severity: Severity;
  kind: "agent" | "printer" | "job";
  reason: AttentionReason;
  mono: string;
  sectionId: string;
  ageMs: number | null;
  titleKey: {
    namespace: string;
    key: string;
    values?: Record<string, string | number>;
  };
  labelKey: {
    namespace: string;
    key: string;
    values?: Record<string, string | number>;
  } | null;
};
```

Update `prettifyToken` and `statusMeta` to take a translator for the `tokens` namespace:

```ts
export type TokenTranslator = (token: string) => string;

export function prettifyToken(
  value: string,
  tokenTranslator?: TokenTranslator,
): string {
  const translated = tokenTranslator?.(value.toLowerCase());
  if (translated) {
    return translated;
  }
  const cleaned = value.replace(/[_-]+/g, " ").trim();
  return cleaned.length
    ? cleaned.charAt(0).toUpperCase() + cleaned.slice(1)
    : value;
}

export function statusMeta(
  value: string,
  tokenTranslator?: TokenTranslator,
): { severity: Severity; label: string } {
  return {
    severity: statusSeverity(value),
    label: prettifyToken(value, tokenTranslator),
  };
}
```

(`tokenTranslator` returns the translated token if known, else `undefined` so the prettify fallback runs. Callers build it as: `const tTokens = useTranslations('tokens'); const tokenTranslator = (k: string) => { try { return tTokens.has(k) ? tTokens(k) : undefined } catch { return undefined } }`.)

Rewrite the `computeAttention` body to set `reason`, `titleKey`, `labelKey`, and drop English `title`/`label`. Use `prettifyToken` only for fallback display values stored inside `labelKey.values`:

```ts
export function computeAttention(args: {
  agents: Agent[];
  printers: Printer[];
  jobs: Job[];
  nowMs: number;
}): AttentionItem[] {
  const { agents, printers, jobs, nowMs } = args;
  const items: AttentionItem[] = [];

  for (const agent of agents) {
    if (!HEALTHY_AGENT_STATUSES.has(agent.status.toLowerCase())) {
      items.push({
        id: `agent:${agent.id}`,
        agentId: agent.id,
        agentName: agent.name,
        severity: statusSeverity(agent.status),
        kind: "agent",
        reason: "agent_unhealthy",
        mono: agent.id,
        sectionId: "printers",
        ageMs: null,
        titleKey: {
          namespace: "attention.agent",
          key: "title",
          values: { status: prettifyToken(agent.status) },
        },
        labelKey: {
          namespace: "attention.agent",
          key: "label",
          values: { name: agent.name, status: agent.status || "offline" },
        },
      });
    }
  }

  for (const printer of printers) {
    if (OFFLINE_PRINTER_STATUSES.has(printer.status.toLowerCase())) {
      items.push({
        id: `printer:${printer.id}`,
        agentId: printer.agent_id,
        agentName: agentName(agents, printer.agent_id),
        severity: statusSeverity(printer.status),
        kind: "printer",
        reason: "printer_offline",
        mono: printer.serial_number,
        sectionId: "printers",
        ageMs: null,
        titleKey: {
          namespace: "attention.printer",
          key: "title",
          values: { status: prettifyToken(printer.status) },
        },
        labelKey: {
          namespace: "attention.printer",
          key: "label",
          values: { name: printer.name, status: printer.status },
        },
      });
    }
  }

  for (const job of jobs) {
    if (isJobFailed(job)) {
      const physical = job.print.status.toLowerCase() === "failed";
      items.push({
        id: `job:${job.id}:failed`,
        agentId: job.agent_id,
        agentName: agentName(agents, job.agent_id),
        severity: statusSeverity(physical ? job.print.status : job.status),
        kind: "job",
        reason: physical ? "job_print_failed" : "job_dispatch_failed",
        mono: job.id,
        sectionId: "recovery",
        ageMs: null,
        titleKey: {
          namespace: physical
            ? "attention.jobPrintFailed"
            : "attention.jobDispatchFailed",
          key: "title",
        },
        labelKey: {
          namespace: "job",
          key: "filename",
          values: { filename: job.artifact.filename },
        },
      });
    } else if (nowMs > 0 && isJobActive(job) && isStale(job, nowMs)) {
      items.push({
        id: `job:${job.id}:stale`,
        agentId: job.agent_id,
        agentName: agentName(agents, job.agent_id),
        severity: "warning",
        kind: "job",
        reason: "job_stalled",
        mono: job.id,
        sectionId: "jobs",
        ageMs: staleAgeMs(job, nowMs),
        titleKey: { namespace: "attention.jobStalled", key: "title" },
        labelKey: {
          namespace: "attention.jobStalled",
          key: "label",
          values: {
            filename: job.artifact.filename,
            duration: formatDuration(
              staleAgeMs(job, nowMs) ?? 0,
              enFallbackDuration,
            ),
          },
        },
      });
    }
  }

  return items.sort((a, b) => {
    if (a.agentName !== b.agentName)
      return a.agentName.localeCompare(b.agentName);
    return SEVERITY_RANK[a.severity] - SEVERITY_RANK[b.severity];
  });
}

const enFallbackDuration: Translator = (key, values) => {
  const count = (values?.count as number) ?? 0;
  if (key === "lessThanMinute") return "less than a minute";
  if (key === "minutes") return `${count} minute${count === 1 ? "" : "s"}`;
  return `${count} hour${count === 1 ? "" : "s"}`;
};
```

Also hoist the `agentName` helper to module scope (it was inline as a closure). Replace the inline closure with:

```ts
function agentName(agents: Agent[], id: string): string {
  return agents.find((agent) => agent.id === id)?.name ?? "";
}
```

(The display translation of "Unknown agent" is applied at render time via the `attention.unknownAgent` key when the name is empty — the rendering task handles that.)

Add a new top-level `job.filename` key to messages (both locales):

`messages/en.json` add top-level:

```json
"job": { "filename": "{filename}" }
```

`messages/zh.json` add:

```json
"job": { "filename": "{filename}" }
```

Also: `formatDuration` is still referenced by the old inline closure signature — now it takes a translator. The `enFallbackDuration` above keeps the `computeAttention` logic locale-independent (it only feeds the stored `labelKey.values.duration`, which is a pre-rendered string for the stale-time span). Rendering tasks can choose to re-render duration via a translator; for now storing the prettified-English duration in `labelKey.values` is acceptable because the duration text is part of a composed label that's itself retranslated. **Correction:** to keep duration translatable, do NOT pre-render it. Instead store the raw ms in values and translate in the message. Update the stale item:

```ts
labelKey: { namespace: 'attention.jobStalled', key: 'labelMs', values: { filename: job.artifact.filename, minutes: Math.round((staleAgeMs(job, nowMs) ?? 0) / 60000) } },
```

and message keys (en): `"label": "{filename} · no progress for {duration}"` (drop), add `"labelMs": "{filename} · no progress for {minutes, plural, =1 {1 minute} other {# minutes}}"`. zh: `"labelMs": "{filename} · {minutes, plural, other {# 分钟}} 无进展"`. Remove the `enFallbackDuration` constant and the `Translator` import here (unused after this correction). The `formatDuration` export in `dashboard-runtime-helpers.ts` remains for other callers.

Apply this correction when writing the file (i.e., the stale branch uses `labelMs` + numeric `minutes`, not the pre-rendered string).

- [ ] **Step 4: Refactor `computeVerdict` in `dashboard-status.tsx`**

`computeVerdict` currently returns English `title`/`detail`. Change it to accept a translator scoped to `overview.verdict` and return translated strings. Replace the function body:

```ts
export function computeVerdict(
  args: {
    attentionCount: number;
    topSeverity: Severity | null;
    liveState: LiveState;
    fleetEmpty: boolean;
  },
  t: (key: string, values?: Record<string, string | number>) => string,
): Verdict {
  const { attentionCount, topSeverity, liveState, fleetEmpty } = args;

  if (fleetEmpty) {
    return {
      title: t("noFleet.title"),
      detail: t("noFleet.detail"),
      severity: "info",
      tone: TONES.info,
    };
  }
  if (liveState === "unavailable" || liveState === "error") {
    return {
      title: t("liveUnavailable.title"),
      detail: t("liveUnavailable.detail"),
      severity: "warning",
      tone: TONES.warning,
    };
  }
  if (liveState === "disconnected") {
    return {
      title: t("liveDisconnected.title"),
      detail: t("liveDisconnected.detail"),
      severity: "warning",
      tone: TONES.warning,
    };
  }
  if (attentionCount === 0) {
    return {
      title: t("nominal.title"),
      detail: t("nominal.detail"),
      severity: "success",
      tone: TONES.success,
    };
  }
  const severity = topSeverity ?? "warning";
  return {
    title: t("needAttention.title", { count: attentionCount }),
    detail:
      severity === "critical"
        ? t("needAttention.detailCritical")
        : t("needAttention.detailOther"),
    severity,
    tone: severity === "critical" ? TONES.critical : TONES.warning,
  };
}
```

Also update `AttentionAction` to switch on `item.reason` instead of `item.title`:

- Replace `if (item.kind === 'job' && item.title === 'Print failed')` with `if (item.kind === 'job' && item.reason === 'job_print_failed')`.
- Replace `if (item.kind === 'job' && item.title === 'Dispatch failed')` with `if (item.kind === 'job' && item.reason === 'job_dispatch_failed')`.

The button labels (`Refresh`, `Reprint`, `Retry dispatch`, `View`) become translated in the `overview` task — leave them as English literals in this step (they compile fine) and replace in the consuming task. **However**, since `dashboard-status.tsx` has no `'use client'` directive and renders buttons, those literals will be replaced in Task 6 anyway. To avoid churn, replace them now with the `overview` translations: add to `messages/en.json` under `overview`:

```json
"action": { "refresh": "Refresh", "reprint": "Reprint", "retryDispatch": "Retry dispatch", "view": "View" }
```

zh:

```json
"action": { "refresh": "刷新", "reprint": "重新打印", "retryDispatch": "重试派发", "view": "查看" }
```

But `AttentionAction` is not a component that currently calls `useTranslations` — leave the literals English in this step and replace in Task 6 to keep this task focused on the signature/type changes. (Task 6 owns all `overview`/`status`/`attention` string rendering.)

- [ ] **Step 5: Refactor `job-format.ts`**

Replace `formatLayers` and `formatRemaining` to take a translator scoped to a new `jobFormat` namespace:

```ts
export type Translator = (
  key: string,
  values?: Record<string, string | number>,
) => string;

export function formatLayers(
  job: PrintJobForFormatting,
  t: Translator,
): string {
  const current = job.print.current_layer ?? job.print.last_layer;
  if (current === null && job.print.total_layers === null) {
    return t("none");
  }
  if (current === null) {
    return t("openTotal", { total: job.print.total_layers ?? "-" });
  }
  if (job.print.total_layers === null) {
    return t("openCurrent", { current });
  }
  return t("both", { current, total: job.print.total_layers });
}

export function formatRemaining(minutes: number | null, t: Translator): string {
  if (minutes === null) {
    return t("none");
  }
  if (minutes < 60) {
    return t("minutes", { minutes });
  }
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return t("hoursMinutes", { hours, rest });
}
```

`formatProgress` stays unchanged (it's `"%"` + a number).

Add `jobFormat` namespace to messages:

`messages/en.json` add:

```json
"jobFormat": {
  "layersNone": "Layers -",
  "layersOpenTotal": "Layers -/{total}",
  "layersOpenCurrent": "Layers {current}",
  "layersBoth": "Layers {current}/{total}",
  "remainingNone": "Remaining -",
  "remainingMinutes": "Remaining {minutes}m",
  "remainingHours": "Remaining {hours}h {rest}m"
}
```

`messages/zh.json` add:

```json
"jobFormat": {
  "layersNone": "层数 -",
  "layersOpenTotal": "层数 -/{total}",
  "layersOpenCurrent": "层数 {current}",
  "layersBoth": "层数 {current}/{total}",
  "remainingNone": "剩余 -",
  "remainingMinutes": "剩余 {minutes} 分钟",
  "remainingHours": "剩余 {hours} 小时 {rest} 分钟"
}
```

(The translator passed by the caller is scoped to `jobFormat`, so the leaf keys are `none`/`openTotal`/`openCurrent`/`both`/`none`/`minutes`/`hoursMinutes`. Adjust the message key names to match exactly: use `none`/`openTotal`/`openCurrent`/`both` for layers, and `none`/`minutes`/`hoursMinutes` for remaining. Collapsing both `none` keys is fine since they share a namespace — but they render different text ("Layers -" vs "Remaining -"). **Keep them distinct:** name them `layersNone`, `layersOpenTotal`, `layersOpenCurrent`, `layersBoth`, `remainingNone`, `remainingMinutes`, `remainingHours` and have the caller call `t('layersNone')` etc. Update the function bodies above to use those full keys.)

Final corrected `job-format.ts` keys used: `layersNone`, `layersOpenTotal`, `layersOpenCurrent`, `layersBoth`, `remainingNone`, `remainingMinutes`, `remainingHours`.

- [ ] **Step 6: Build to verify type changes compile**

Callers of the changed helpers now pass wrong arg counts (they will be fixed in Tasks 5–13). To keep the build green at this checkpoint, update the **direct callers** of the changed helpers to pass a temporary English-only identity translator so types line up, with a `// TODO(i18n): replace with useTranslations in Task N` marker is NOT allowed (no comments). Instead, the cleanest checkpoint: do this task as a pure **signature change with a default no-op translator** so existing callers still compile.

**Revised approach for build-green checkpoints:** give every changed helper a defaulted translator that reproduces the current English output, so callers that haven't been migrated yet continue to compile and render English. Migrated callers in later tasks pass a real translator.

For example in `dashboard-runtime-helpers.ts`:

```ts
const enLiveState: Record<LiveState, string> = {
  live: "Connected",
  connecting: "Connecting",
  disconnected: "Reconnecting",
  idle: "Idle",
  unavailable: "Unavailable",
  error: "Unavailable",
};
export function formatLiveState(
  state: LiveState,
  t: Translator = (k) => enLiveState[state],
): string {
  switch (state /* same as Step 2 */) {
  }
}
```

Apply the same default-translator pattern to `formatAuthSource`, `formatJobRecoveryState`, `formatDuration`, `formatPrinterMaterials` (default `formatDate` = existing impl), `formatJobMaterial`, `formatArtifactMetadata`, `computeVerdict` (default `t` = existing English map), `formatLayers`/`formatRemaining` (default `t` = existing English map), `prettifyToken`/`statusMeta` (already optional in Step 3).

Concretely, each helper's default translator reproduces the **pre-refactor English output**. Provide those default maps inline at the helper. This keeps the public behavior identical until callers migrate.

- [ ] **Step 7: Build**

Run: `cd frontend && npm run lint && npm run build`
Expected: succeeds; app behavior unchanged (still all English) because default translators reproduce prior output.

- [ ] **Step 8: Commit (if user permits)**

```bash
git add frontend/app/dashboard-runtime-helpers.ts frontend/app/dashboard-attention.ts frontend/app/dashboard-status.tsx frontend/app/job-format.ts frontend/app/dashboard-ui.tsx frontend/messages/en.json frontend/messages/zh.json
git commit -m "refactor(frontend): make string builders locale-aware with default translators"
```

---

## Task 5: Translate header + nav; place LanguageSwitcher

**Files:**

- Modify: `frontend/app/dashboard-header.tsx`
- Modify: `frontend/app/dashboard-overview.tsx` (NAV_SECTIONS only — full overview translation is Task 6)
- Modify: `frontend/app/dashboard-ui.tsx` (add switcher to `SectionHeader`)
- Modify: `frontend/messages/en.json`, `frontend/messages/zh.json`

**Interfaces:** consumes Task 1 provider + Task 2 `<LanguageSwitcher />`.

- [ ] **Step 1: Add `header` and `nav` namespaces**

`messages/en.json` add:

```json
"nav": {
  "printers": "Printers",
  "jobs": "Print jobs",
  "dispatch": "Dispatch",
  "recovery": "Recovery",
  "diagnostics": "Diagnostics",
  "activity": "Live activity",
  "admin": "Admin"
},
"header": {
  "title": "Pandar Operations",
  "inventoryFrom": "Tenant printer inventory from {apiUrl}",
  "tenant": "Tenant",
  "view": "View"
}
```

`messages/zh.json` add:

```json
"nav": {
  "printers": "打印机",
  "jobs": "打印任务",
  "dispatch": "派发",
  "recovery": "恢复",
  "diagnostics": "诊断",
  "activity": "实时活动",
  "admin": "管理"
},
"header": {
  "title": "Pandar 运维控制台",
  "inventoryFrom": "来自 {apiUrl} 的租户打印机清单",
  "tenant": "租户",
  "view": "查看"
}
```

- [ ] **Step 2: Refactor `dashboard-header.tsx`**

Add `import { useTranslations } from 'next-intl'` and `import { LanguageSwitcher } from '../components/language-switcher'`. Inside `Header`, `const t = useTranslations('header')`. Replace:

- `"Pandar Operations"` → `{t('title')}`
- `` `Tenant printer inventory from ${apiUrl}` `` → `{t('inventoryFrom', { apiUrl })}`
- `"Tenant"` → `{t('tenant')}`
- `"View"` → `{t('view')}`

Add `<LanguageSwitcher />` inside the header next to the tenant form (or in the title row when there is only one tenant). Minimal placement: put it in the title `<div>` after the `<p>`:

```tsx
<div>
  <h1 className="text-2xl font-semibold">{t("title")}</h1>
  <p className="mt-1 text-sm text-slate-600">
    {t("inventoryFrom", { apiUrl })}
  </p>
  <div className="mt-2">
    <LanguageSwitcher />
  </div>
</div>
```

- [ ] **Step 3: Make `NAV_SECTIONS` translation-driven in `dashboard-overview.tsx`**

Change `NAV_SECTIONS` to IDs only and translate labels in `SectionNav`. Replace:

```ts
export type NavSection = { id: string; label: string }
export const NAV_SECTIONS: NavSection[] = [ ... ]
```

with:

```ts
export const NAV_SECTION_IDS = [
  "printers",
  "jobs",
  "dispatch",
  "recovery",
  "diagnostics",
  "activity",
  "admin",
] as const;
export type NavSectionId = (typeof NAV_SECTION_IDS)[number];
```

In `SectionNav`, `const tNav = useTranslations('nav')`, and map over `NAV_SECTION_IDS`, rendering `{tNav(section)}`. (Other strings in `dashboard-overview.tsx` are handled in Task 6.)

- [ ] **Step 4: Add switcher to `SectionHeader` in `dashboard-ui.tsx`**

`dashboard-ui.tsx` already has `'use client'`. Add `import { LanguageSwitcher } from '../components/language-switcher'`. Render it in the `meta` cell of `SectionHeader`:

```tsx
<div className="flex items-center gap-2 text-sm text-slate-600">
  <LanguageSwitcher />
  <span>{meta}</span>
</div>
```

(This puts the switcher on every standalone page that uses `SectionHeader`: onboarding, sign-in, join, plugin-sign-in.)

- [ ] **Step 5: Build + smoke**

Run: `cd frontend && npm run lint && npm run build`. Open `/`: the header shows translations when the cookie is `zh`; the switcher toggles between EN/中文 and the page re-renders on click. NAV labels translate.

- [ ] **Step 6: Commit (if user permits)**

```bash
git add frontend/app/dashboard-header.tsx frontend/app/dashboard-overview.tsx frontend/app/dashboard-ui.tsx frontend/messages/en.json frontend/messages/zh.json
git commit -m "feat(frontend): translate header and nav, place language switcher"
```

---

## Task 6: Translate overview, status, attention (rendering layer)

**Files:**

- Modify: `frontend/app/dashboard-overview.tsx` (FleetStatusStrip, NeedsAttention, StatCell usage, AttentionRow)
- Modify: `frontend/app/dashboard-status.tsx` (AttentionAction button literals; StatCell stays presentational)
- Modify: `frontend/app/dashboard-ui.tsx` (`StatusBadge` uses `statusMeta` with token translator)
- Modify: `frontend/messages/en.json`, `frontend/messages/zh.json`

**Interfaces:** consumes Task 4 refactored helpers (`computeVerdict(args, t)`, `AttentionItem.reason`/`titleKey`/`labelKey`, `prettifyToken(value, tokenTranslator)`, `statusMeta(value, tokenTranslator)`).

- [ ] **Step 1: Add the `overview.status`, `overview.attention` namespaces**

`messages/en.json` under `overview` (merge with existing `overview.verdict`, `overview.action`):

```json
"stat": {
  "printers": "Printers",
  "printersValue": "{online}/{total} online",
  "printersNote": "{count} offline",
  "agents": "Agents",
  "agentsValue": "{connected}/{total} connected",
  "agentsNote": "{count} down",
  "activeJobs": "Active jobs",
  "activeJobsValue": "{count} active",
  "activeJobsNote": "{count} failed",
  "dash": "—"
},
"attentionTitle": "Needs attention",
"attentionSubtitle": "{count, plural, =1 {# exception} other {# exceptions}} across the fleet",
"groupedByAgent": "Grouped by agent",
"ariaFleet": "Fleet status",
"ariaAttention": "Needs attention",
"ariaSections": "Sections"
```

`messages/zh.json` under `overview`:

```json
"stat": {
  "printers": "打印机",
  "printersValue": "{online}/{total} 在线",
  "printersNote": "{count} 离线",
  "agents": "Agent",
  "agentsValue": "{connected}/{total} 已连接",
  "agentsNote": "{count} 下线",
  "activeJobs": "进行中任务",
  "activeJobsValue": "{count} 个进行中",
  "activeJobsNote": "{count} 个失败",
  "dash": "—"
},
"attentionTitle": "需要关注",
"attentionSubtitle": "机队中共 {count, plural, other {# 项异常}}",
"groupedByAgent": "按 Agent 分组",
"ariaFleet": "机队状态",
"ariaAttention": "需要关注",
"ariaSections": "分区"
```

- [ ] **Step 2: Render `FleetStatusStrip` with translations**

In `dashboard-overview.tsx`, inside `FleetStatusStrip` add `const t = useTranslations('overview.verdict')` and `const tStat = useTranslations('overview.stat')` and `const tAria = useTranslations('overview')`. Pass `t` to `computeVerdict({...}, t)`. Replace the three `<StatCell .../>` strings:

- printers: `label={tStat('printers')}`, `value={fleetEmpty ? tStat('dash') : tStat('printersValue', { online: health.printersOnline, total: health.printersTotal })}`, `note={... ? tStat('printersNote', { count: health.printersTotal - health.printersOnline }) : null}`.
- agents: analogous with `agentsValue`/`agentsNote`.
- jobs: `activeJobs`/`activeJobsValue`/`activeJobsNote`.

Update `aria-label="Fleet status"` → `aria-label={tAria('ariaFleet')}`.

- [ ] **Step 3: Render `NeedsAttention` and `AttentionRow`**

In `NeedsAttention`: `const tAtt = useTranslations('overview')`. Replace `"Needs attention"` → `{tAtt('attentionTitle')}`, the subtitle expression → `{tAtt('attentionSubtitle', { count: items.length })}`, `"Grouped by agent"` → `{tAtt('groupedByAgent')}`, `aria-label="Needs attention"` → `aria-label={tAtt('ariaAttention')}`.

`AttentionRow` renders `item.title`/`item.label`. These are now locale-neutral descriptors. Resolve them with next-intl's `useTranslations` by namespace. Because the namespace is dynamic, use `useTranslations()` (root) and a helper:

```tsx
function useResolvedText(key: {
  namespace: string;
  key: string;
  values?: Record<string, string | number>;
}) {
  const t = useTranslations(key.namespace);
  return t(key.key, key.values);
}
```

Place this hook at module scope in `dashboard-status.tsx` (where `AttentionRow` lives). In `AttentionRow`:

```tsx
const title = useResolvedText(item.titleKey);
const label = item.labelKey ? useResolvedText(item.labelKey) : "";
```

Replace `{item.title}` → `{title}`, `{item.label}` → `{label || item.mono}`. (The `labelKey` for jobs is `job.filename` which echoes `{filename}`.)

- [ ] **Step 4: Translate `AttentionAction` button literals in `dashboard-status.tsx`**

`AttentionAction` currently renders `"View"`, `"Refresh"`, `"Reprint"`, `"Retry dispatch"`. Add `const tAct = useTranslations('overview.action')` and replace each literal with `{tAct('view')}` / `{tAct('refresh')}` / `{tAct('reprint')}` / `{tAct('retryDispatch')}`. Confirm the `reason`-based branching from Task 4 Step 4 is in place.

- [ ] **Step 5: Token-translate `StatusBadge` in `dashboard-ui.tsx`**

In `StatusBadge`, build the token translator and pass to `statusMeta`:

```tsx
import { useTranslations } from "next-intl";
// inside StatusBadge:
const tTokens = useTranslations("tokens");
const tokenTranslator = (k: string) =>
  tTokens.has(k) ? tTokens(k) : undefined;
const { severity, label } = statusMeta(value, tokenTranslator);
```

(`tTokens.has` is supported by next-intl.) `Tag` uses `prettifyToken` — update similarly: `prettifyToken(value, tokenTranslator)`. Since `Tag` is generic (renders arbitrary tokens like roles/scopes), pass the same translator.

- [ ] **Step 6: Build + smoke**

Run: `cd frontend && npm run lint && npm run build`. Smoke: with `locale=zh`, the fleet strip, "Needs attention" block, status badges, and attention-row actions render in Chinese; switching to EN reverts. Failed-job attention rows still show "Reprint"/"Retry dispatch" actions correctly (proving the `reason` discriminator works post-translation).

- [ ] **Step 7: Commit (if user permits)**

```bash
git add frontend/app/dashboard-overview.tsx frontend/app/dashboard-status.tsx frontend/app/dashboard-ui.tsx frontend/messages/en.json frontend/messages/zh.json
git commit -m "feat(frontend): translate overview, status, and attention rendering"
```

---

## Task 7: Translate inventory + job-format rendering

**Files:**

- Modify: `frontend/app/dashboard-inventory.tsx`
- Modify: `frontend/messages/en.json`, `frontend/messages/zh.json`

**Interfaces:** consumes Task 3 `<FormattedDate />`, Task 4 `formatPrinterMaterials(printer, t, formatDate)` / `formatArtifactMetadata(job, t, formatDate)` / `formatJobMaterial(job, t)` / `formatLayers(job, t)` / `formatRemaining(minutes, t)`.

- [ ] **Step 1: Add `inventory` namespace**

`messages/en.json` add:

```json
"inventory": {
  "printersTitle": "Printer inventory",
  "printersSubtitleTenant": "{name} ({slug})",
  "printersSubtitleNone": "No tenant selected",
  "printersMeta": "{count} reported",
  "noTenantTitle": "No tenants",
  "noTenantMessage": "Ask your administrator to create a tenant and assign you to it, then select it in the header. Printers appear here once an agent reports them.",
  "noPrintersTitle": "No printers reported",
  "noPrintersMessage": "Connect an agent and run a printer refresh to populate this inventory.",
  "searchName": "Search name or serial",
  "filterAll": "All statuses",
  "filterOnline": "Online",
  "filterAttention": "Needs attention",
  "noMatchesTitle": "No matches",
  "noMatchesMessage": "No printers match your search or filter.",
  "unknownModel": "Unknown model",
  "managedBy": "Managed by",
  "unknownAgent": "Unknown agent",
  "filterStatusAria": "Filter by status",
  "jobsTitle": "Print jobs",
  "jobsSubtitle": "Queued, dispatched, and physical print history",
  "jobsMeta": "{count} jobs",
  "jobsNoTenantTitle": "No tenant selected",
  "jobsNoTenantMessage": "Select a tenant to inspect jobs.",
  "jobsEmptyTitle": "No jobs",
  "jobsEmptyMessage": "Dispatch a project file from the Dispatch section to a printer to create your first print job.",
  "searchJob": "Search filename or job id",
  "jobFilterAll": "All jobs",
  "jobFilterActive": "Active",
  "jobFilterFailed": "Failed",
  "jobFilterCompleted": "Completed",
  "jobsNoMatchesTitle": "No matches",
  "jobsNoMatchesMessage": "No jobs match your search or filter.",
  "jobsAria": "Print jobs",
  "updated": "Updated {date}",
  "dispatch": "Dispatch",
  "print": "Print",
  "unknownPrinter": "Unknown printer",
  "details": "Details",
  "recoveryLabel": "Recovery:",
  "projectLabel": "Project:",
  "artifactLabel": "Artifact:",
  "materialLabel": "Material:",
  "jobLabel": "Job:",
  "fileLabel": "File:",
  "stateLabel": "State:",
  "createdLabel": "Created:",
  "startedLabel": "Started:",
  "finishedLabel": "Finished:"
}
```

`messages/zh.json` add:

```json
"inventory": {
  "printersTitle": "打印机清单",
  "printersSubtitleTenant": "{name} ({slug})",
  "printersSubtitleNone": "未选择租户",
  "printersMeta": "已上报 {count} 台",
  "noTenantTitle": "无租户",
  "noTenantMessage": "请联系管理员创建租户并将您加入，然后在页头中选择。Agent 上报打印机后会显示在此处。",
  "noPrintersTitle": "暂无打印机上报",
  "noPrintersMessage": "请连接 Agent 并执行打印机刷新以填充清单。",
  "searchName": "搜索名称或序列号",
  "filterAll": "全部状态",
  "filterOnline": "在线",
  "filterAttention": "需要关注",
  "noMatchesTitle": "无匹配结果",
  "noMatchesMessage": "没有打印机匹配您的搜索或筛选。",
  "unknownModel": "未知型号",
  "managedBy": "管理者",
  "unknownAgent": "未知 Agent",
  "filterStatusAria": "按状态筛选",
  "jobsTitle": "打印任务",
  "jobsSubtitle": "排队、派发与实际打印历史",
  "jobsMeta": "{count} 个任务",
  "jobsNoTenantTitle": "未选择租户",
  "jobsNoTenantMessage": "请选择一个租户以查看任务。",
  "jobsEmptyTitle": "暂无任务",
  "jobsEmptyMessage": "在“派发”区域向打印机派发一个项目文件以创建首个打印任务。",
  "searchJob": "搜索文件名或任务 ID",
  "jobFilterAll": "全部任务",
  "jobFilterActive": "进行中",
  "jobFilterFailed": "失败",
  "jobFilterCompleted": "已完成",
  "jobsNoMatchesTitle": "无匹配结果",
  "jobsNoMatchesMessage": "没有任务匹配您的搜索或筛选。",
  "jobsAria": "打印任务",
  "updated": "更新于 {date}",
  "dispatch": "派发",
  "print": "打印",
  "unknownPrinter": "未知打印机",
  "details": "详情",
  "recoveryLabel": "恢复：",
  "projectLabel": "项目：",
  "artifactLabel": "产物：",
  "materialLabel": "耗材：",
  "jobLabel": "任务：",
  "fileLabel": "文件：",
  "stateLabel": "状态：",
  "createdLabel": "创建：",
  "startedLabel": "开始：",
  "finishedLabel": "结束："
}
```

- [ ] **Step 2: Wire translators into `dashboard-inventory.tsx`**

In `PrinterInventory`: `const t = useTranslations('inventory')`, `const tMat = useTranslations('material')`, `const tRec = useTranslations('recovery.state')`. Build a date formatter bound to the locale:

```tsx
import { useFormatter } from "next-intl";
const format = useFormatter();
const formatDate = (value: string) => {
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return value;
  return format.dateTime(d, {
    dateStyle: "medium",
    timeStyle: "short",
    timeZone: "UTC",
  });
};
```

Replace:

- `SectionHeader title/subtitle/meta` → `t('printersTitle')`, `selectedTenant ? t('printersSubtitleTenant', { name, slug }) : t('printersSubtitleNone')`, `t('printersMeta', { count })`.
- Empty states → corresponding `t(...)`.
- `queryPlaceholder="Search name or serial"` → `t('searchName')` (also the `aria-label`).
- `statusOptions` labels → `t('filterAll')`, `t('filterOnline')`, `t('filterAttention')`.
- `"No matches"` → `t('noMatchesTitle')`/`t('noMatchesMessage')`.
- `'Unknown model'` → `t('unknownModel')`.
- `formatDate(printer.last_seen_at)` (JSX) → `<FormattedDate value={printer.last_seen_at} />`.
- `material.summary`/`material.detail` → `formatPrinterMaterials(printer, tMat, formatDate)`.
- `"Managed by"` → `t('managedBy')`, `'Unknown agent'` → `t('unknownAgent')`.

In `JobHistory`: same translator pattern plus `const tJf = useTranslations('jobFormat')`. Replace SectionHeader, empty states, filter labels, aria. In `JobRow`:

- `aria-label={...}` composition: `t('dispatch')`/`t('print')` etc. plus `formatProgress`.
- `"Updated {date}"` → `t('updated', { date: <raw> })` — but `t()` returns a string, cannot embed JSX. Instead render: `<span>{t('updated', { date: '' }).replace(/\s*$/, '')} </span><FormattedDate value={updated} />`. Simpler: split into two spans: `<span className="text-slate-500">{t('updatedPrefix')}</span> <FormattedDate value={updated} />` with key `"updatedPrefix": "Updated"`. **Use the split approach:** add `updatedPrefix`/`updatedPrefixZh`? No — single key `updatedPrefix` in both locales (`en: "Updated"`, `zh: "更新于"`). Update message files accordingly (replace `updated` key with `updatedPrefix`).
- `StatusPill` labels `"Dispatch"`/`"Print"` → `t('dispatch')`/`t('print')`.
- `'Unknown printer'`/`'Unknown agent'` → `t('unknownPrinter')`/`t('unknownAgent')`.
- `formatProgress(job)` stays (number + `%`).
- `formatLayers(job)` → `formatLayers(job, tJf)`; `formatRemaining(...)` → `formatRemaining(job.print.remaining_time_minutes, tJf)`.
- `<summary>Details</summary>` → `{t('details')}`.
- Detail labels (`Recovery:`/`Project:`/`Artifact:`/`Material:`/`Job:`/`File:`/`State:`/`Created:`/`Started:`/`Finished:`) → corresponding `t(...)`.
- `formatJobRecoveryState(job)` → `formatJobRecoveryState(job, tRec)`; `formatArtifactMetadata(job, ...)` → `formatArtifactMetadata(job, tMat, formatDate)`; `formatJobMaterial(job, ...)` → `formatJobMaterial(job, tMat)`.
- All `formatDate(...)` JSX calls → `<FormattedDate value={...} />`.

In `FilterBar`: replace `aria-label="Filter by status"` → `t('filterStatusAria')` (passed from parent or via its own hook; `FilterBar` is a local component — give it its own `useTranslations('inventory')`).

- [ ] **Step 3: Build + smoke**

Run: `cd frontend && npm run lint && npm run build`. Smoke `locale=zh`: printer inventory and job history fully Chinese; dates formatted zh-style; details panel labels translated; switching EN reverts.

- [ ] **Step 4: Commit (if user permits)**

```bash
git add frontend/app/dashboard-inventory.tsx frontend/messages/en.json frontend/messages/zh.json
git commit -m "feat(frontend): translate printer inventory and job history"
```

---

## Task 8: Translate dispatch form

**Files:**

- Modify: `frontend/app/dispatch-form.tsx`
- Modify: `frontend/messages/en.json`, `frontend/messages/zh.json`

- [ ] **Step 1: Add `dispatch` namespace**

`messages/en.json` add:

```json
"dispatch": {
  "title": "Dispatch print job",
  "subtitle": "Upload a project artifact to the selected tenant printer",
  "noTenantTitle": "No tenant selected",
  "noTenantMessage": "Select a tenant to dispatch jobs.",
  "noPrintersTitle": "No printers available",
  "noPrintersMessage": "A reported printer is required before jobs can be dispatched.",
  "printer": "Printer",
  "plate": "Plate",
  "plateHelp": "Which plate from the project file to print. Use 1 if the file has a single plate.",
  "artifact": "Artifact",
  "maxSize": "Maximum artifact size {size}",
  "noArtifact": "No artifact selected",
  "readySize": "{size} selected",
  "tooLargeSize": "{size} exceeds the configured limit",
  "chooseFile": "Choose a file before dispatch.",
  "errorCodes": "Developer error codes",
  "useAms": "Use AMS",
  "useAmsHelp": "Use the printer's AMS units to pick filament for each part of the print.",
  "flowCali": "Flow calibration",
  "flowCaliHelp": "Run flow dynamics calibration first. Improves extrusion accuracy but adds time before the print.",
  "timelapse": "Timelapse",
  "timelapseHelp": "Record a timelapse of the print with the printer's camera.",
  "dispatching": "Dispatching",
  "dispatch": "Dispatch",
  "readingMetadata": "Reading slicer metadata",
  "metadataUnavailableFound": "No slicer metadata found",
  "metadataUnavailable": "Metadata preview unavailable",
  "project": "Project",
  "plateLabel": "Plate",
  "objects": "Objects"
}
```

`messages/zh.json` add:

```json
"dispatch": {
  "title": "派发打印任务",
  "subtitle": "将项目产物上传到所选租户打印机",
  "noTenantTitle": "未选择租户",
  "noTenantMessage": "请选择一个租户以派发任务。",
  "noPrintersTitle": "暂无可用打印机",
  "noPrintersMessage": "派发任务前需要先有一台上报的打印机。",
  "printer": "打印机",
  "plate": "盘子",
  "plateHelp": "指定要打印项目文件中的哪个盘子。单盘子文件请填 1。",
  "artifact": "产物",
  "maxSize": "产物最大尺寸 {size}",
  "noArtifact": "未选择产物",
  "readySize": "已选择 {size}",
  "tooLargeSize": "{size} 超出配置上限",
  "chooseFile": "派发前请先选择文件。",
  "errorCodes": "开发者错误码",
  "useAms": "使用 AMS",
  "useAmsHelp": "使用打印机的 AMS 单元为打印的每个部件挑选耗材。",
  "flowCali": "流量校准",
  "flowCaliHelp": "先执行流量动态校准。可提升挤出精度，但会增加打印前的耗时。",
  "timelapse": "延时摄影",
  "timelapseHelp": "使用打印机摄像头为本次打印录制延时视频。",
  "dispatching": "派发中",
  "dispatch": "派发",
  "readingMetadata": "正在读取切片元数据",
  "metadataUnavailableFound": "未找到切片元数据",
  "metadataUnavailable": "元数据预览不可用",
  "project": "项目",
  "plateLabel": "盘子",
  "objects": "对象"
}
```

- [ ] **Step 2: Wire `dispatch-form.tsx`**

`const t = useTranslations('dispatch')`, `const format = useFormatter()`. Replace each literal with the matching `t(...)`. For `formatBytes(maxArtifactBytes)` and `formatBytes(artifact.size)`, pass the localized number formatter: `formatBytes(value, (n) => format.number(n))`. `aria-label="Plate"` → `t('plate')`. `MetadataPreview` gets its own `const t = useTranslations('dispatch')` and replaces `"Reading slicer metadata"` etc. `DispatchEmptyState` is local — its `title`/`message` props are now translated strings passed by the caller.

- [ ] **Step 3: Build + smoke**

Run: `cd frontend && npm run lint && npm run build`. Smoke: dispatch form fully translated; file-size strings localized.

- [ ] **Step 4: Commit (if user permits)**

```bash
git add frontend/app/dispatch-form.tsx frontend/messages/en.json frontend/messages/zh.json
git commit -m "feat(frontend): translate dispatch form"
```

---

## Task 9: Translate recovery actions

**Files:**

- Modify: `frontend/app/recovery-actions.tsx`
- Modify: `frontend/messages/en.json`, `frontend/messages/zh.json`

- [ ] **Step 1: Add `recoveryPage` namespace (recovery.state/duration already exist from Task 4)**

`messages/en.json` add:

```json
"recoveryPage": {
  "title": "Recovery actions",
  "subtitle": "Refresh, dispatch retry, reprint, live print controls, and duplicate — shown per job state",
  "meta": "{count} jobs",
  "noTenantTitle": "No tenant selected",
  "noTenantMessage": "Select a tenant to run recovery actions.",
  "noAgentsRefresh": "No agents available for manual refresh",
  "refreshAllAgents": "Refresh all agents",
  "refreshAgent": "Refresh {name}",
  "noJobsTitle": "No jobs",
  "noJobsMessage": "Jobs will appear here when dispatch history exists.",
  "selectedOfFailed": "{selected} of {failed} failed selected",
  "failedCount": "{failed} failed",
  "selectAll": "Select all",
  "deselectAll": "Deselect all",
  "retrySelected": "Retry {count} selected",
  "selectJobAria": "Select {filename}",
  "retryDispatch": "Retry dispatch",
  "reprint": "Reprint",
  "reasonPlaceholder": "reason",
  "samePrinter": "Same printer",
  "platePlaceholder": "plate",
  "duplicate": "Duplicate",
  "printerUnavailable": "Printer record unavailable for live controls",
  "liveUnavailable": "Live controls unavailable for unknown printer model",
  "queuePause": "Queue pause",
  "queueResume": "Queue resume",
  "queueStop": "Queue stop",
  "stopTitle": "Stop print",
  "stopMessage": "Stop this print? The current job cannot be resumed from where it stops.",
  "stopConfirm": "Stop print",
  "silent": "Silent",
  "standard": "Standard",
  "sport": "Sport",
  "ludicrous": "Ludicrous",
  "queueSpeed": "Queue speed"
}
```

`messages/zh.json` add:

```json
"recoveryPage": {
  "title": "恢复操作",
  "subtitle": "刷新、派发重试、重新打印、实时打印控制与复制——按任务状态展示",
  "meta": "{count} 个任务",
  "noTenantTitle": "未选择租户",
  "noTenantMessage": "请选择一个租户以执行恢复操作。",
  "noAgentsRefresh": "没有可手动刷新的 Agent",
  "refreshAllAgents": "刷新全部 Agent",
  "refreshAgent": "刷新 {name}",
  "noJobsTitle": "暂无任务",
  "noJobsMessage": "派发历史产生后会在此显示任务。",
  "selectedOfFailed": "已选 {selected} / {failed} 个失败",
  "failedCount": "{failed} 个失败",
  "selectAll": "全选",
  "deselectAll": "取消全选",
  "retrySelected": "重试已选 {count} 个",
  "selectJobAria": "选择 {filename}",
  "retryDispatch": "重试派发",
  "reprint": "重新打印",
  "reasonPlaceholder": "原因",
  "samePrinter": "同一打印机",
  "platePlaceholder": "盘子",
  "duplicate": "复制",
  "printerUnavailable": "无打印机记录，无法实时控制",
  "liveUnavailable": "未知打印机型号，无法实时控制",
  "queuePause": "排队暂停",
  "queueResume": "排队恢复",
  "queueStop": "排队停止",
  "stopTitle": "停止打印",
  "stopMessage": "停止本次打印？当前任务将无法从停止处恢复。",
  "stopConfirm": "停止打印",
  "silent": "静音",
  "standard": "标准",
  "sport": "运动",
  "ludicrous": "狂飙",
  "queueSpeed": "排队变速"
}
```

- [ ] **Step 2: Wire `recovery-actions.tsx`**

`const t = useTranslations('recoveryPage')`, `const tRec = useTranslations('recovery.state')`, `const tMat = useTranslations('material')`, `const format = useFormatter()`, `formatDate` bound fn. Replace literals:

- SectionHeader title/subtitle/meta.
- Empty states.
- `"No agents available for manual refresh"`, `"Refresh all agents"`, `` `Refresh ${agent.name}` `` → `t('refreshAgent', { name })`.
- Failed selection summary expressions → `t('selectedOfFailed', {...})` / `t('failedCount', { failed })`.
- `"Select all"`/`"Deselect all"`, `` `Retry ${selected.size} selected` `` → `t('retrySelected', { count })`.
- `aria-label={`Select ${job.artifact.filename}`}` → `t('selectJobAria', { filename })`.
- `formatArtifactMetadata(job, ...)` → pass `tMat` + `formatDate`; `formatJobRecoveryState(job, ...)` → pass `tRec`.
- `ReasonForm` `label` prop values `"Retry dispatch"`/`"Reprint"` → `t(...)`. `placeholder="reason"` → `t('reasonPlaceholder')`.
- `DuplicateForm`: `"Same printer"` → `t('samePrinter')`, `placeholder="plate"` → `t('platePlaceholder')`, `"Duplicate"` → `t('duplicate')`.
- `LiveControlPanel`: the two unavailable messages; `PrinterControlForm` `label` props (`"Queue pause"`/`"Queue resume"`); `ConfirmForm` `buttonLabel="Queue stop"`, `title`, `message`, `confirmLabel`; speed-mode option labels (`Silent`/`Standard`/`Sport`/`Ludicrous`); `"Queue speed"` button.

- [ ] **Step 3: Build + smoke**

Run: `cd frontend && npm run lint && npm run build`. Smoke: recovery section fully translated in zh; live-control speed modes translated.

- [ ] **Step 4: Commit (if user permits)**

```bash
git add frontend/app/recovery-actions.tsx frontend/messages/en.json frontend/messages/zh.json
git commit -m "feat(frontend): translate recovery actions"
```

---

## Task 10: Translate diagnostics panel

**Files:**

- Modify: `frontend/app/diagnostics-panel.tsx`
- Modify: `frontend/messages/en.json`, `frontend/messages/zh.json`

- [ ] **Step 1: Add `diagnostics` namespace**

`messages/en.json` add:

```json
"diagnostics": {
  "agentsTitle": "Linked agents",
  "agentsSubtitleTenant": "{name} ({slug})",
  "agentsSubtitleNone": "No tenant selected",
  "agentsMeta": "{count} linked",
  "noTenantTitle": "No tenant selected",
  "noTenantMessage": "Select a tenant to inspect agents.",
  "noAgentsTitle": "No agents linked",
  "noAgentsMessage": "Create an agent pairing before running discovery.",
  "colAgent": "Agent",
  "colStatus": "Status",
  "colCreated": "Created",
  "colDiscovery": "Discovery",
  "timeout": "Timeout",
  "discover": "Discover",
  "title": "Discovery and diagnostics",
  "noCommand": "No command selected",
  "noCommandTitle": "No command selected",
  "noCommandMessage": "Run discovery or diagnostics to inspect the latest structured result.",
  "noStructuredTitle": "No structured result",
  "noStructuredMessage": "The selected command has not returned result data.",
  "diagnose": "Diagnose",
  "noPrintersDiscoveredTitle": "No printers discovered",
  "noPrintersDiscoveredMessage": "Discovery completed with no SSDP responses.",
  "colName": "Name",
  "colSerial": "Serial",
  "colHost": "Host",
  "colModel": "Model",
  "colSource": "Source",
  "colCheck": "Check",
  "colMessage": "Message",
  "colDetails": "Details",
  "compatibility": "Compatibility",
  "model": "Model",
  "externalStorage": "External storage",
  "externalStorageHelp": "Whether the printer can read print files from its SD card or external storage.",
  "ftpsCap": "FTPS TLS 1.2 cap",
  "ftpsCapHelp": "Printer firmware caps FTPS at TLS 1.2. The agent uses a compatible TLS profile when available.",
  "clearDataFallback": "Clear-data fallback",
  "clearDataFallbackHelp": "Whether the agent can fall back to clear-data FTPS transfer for this model family.",
  "available": "Available",
  "unavailable": "Unavailable",
  "unknown": "unknown"
}
```

`messages/zh.json` add:

```json
"diagnostics": {
  "agentsTitle": "已连接 Agent",
  "agentsSubtitleTenant": "{name} ({slug})",
  "agentsSubtitleNone": "未选择租户",
  "agentsMeta": "已连接 {count} 个",
  "noTenantTitle": "未选择租户",
  "noTenantMessage": "请选择一个租户以查看 Agent。",
  "noAgentsTitle": "未连接 Agent",
  "noAgentsMessage": "运行发现前请先创建 Agent 配对。",
  "colAgent": "Agent",
  "colStatus": "状态",
  "colCreated": "创建时间",
  "colDiscovery": "发现",
  "timeout": "超时",
  "discover": "发现",
  "title": "发现与诊断",
  "noCommand": "未选择命令",
  "noCommandTitle": "未选择命令",
  "noCommandMessage": "运行发现或诊断以查看最新的结构化结果。",
  "noStructuredTitle": "无结构化结果",
  "noStructuredMessage": "所选命令尚未返回结果数据。",
  "diagnose": "诊断",
  "noPrintersDiscoveredTitle": "未发现打印机",
  "noPrintersDiscoveredMessage": "发现完成，未收到任何 SSDP 响应。",
  "colName": "名称",
  "colSerial": "序列号",
  "colHost": "主机",
  "colModel": "型号",
  "colSource": "来源",
  "colCheck": "检查项",
  "colMessage": "消息",
  "colDetails": "详情",
  "compatibility": "兼容性",
  "model": "型号",
  "externalStorage": "外部存储",
  "externalStorageHelp": "打印机是否可从 SD 卡或外部存储读取打印文件。",
  "ftpsCap": "FTPS TLS 1.2 上限",
  "ftpsCapHelp": "打印机固件将 FTPS 限制在 TLS 1.2。Agent 会在可用时使用兼容的 TLS 配置。",
  "clearDataFallback": "明文数据回退",
  "clearDataFallbackHelp": "Agent 是否可针对该型号系列回退到明文 FTPS 传输。",
  "available": "可用",
  "unavailable": "不可用",
  "unknown": "未知"
}
```

- [ ] **Step 2: Wire `diagnostics-panel.tsx`**

Note: `diagnostics-panel.tsx` has no `'use client'` — it is rendered inside client components (`LinkedAgentsSection`/`DiagnosticsSection` are imported by `dashboard-runtime.tsx` which is `'use client'`). next-intl `useTranslations` works in either; add `import { useTranslations, useFormatter } from 'next-intl'` and `import { FormattedDate } from '../components/formatted-date'`. In each component (`LinkedAgentsSection`, `DiagnosticsSection`, `DiscoveryResult`, `DiagnosticResult`, `CompatibilityRow`) call `useTranslations('diagnostics')`. Replace literals; replace `formatDate(...)` JSX → `<FormattedDate value={...} />`. Replace `Tag value={available ? 'Available' : 'Unavailable'}` — `Tag` prettifies; instead pass the translated string directly: `Tag value={available ? t('available') : t('unavailable')}` and skip token translation for these (they're already translated). `formatCapabilityName` stays (renders feature key names, locale-neutral identifiers). The `unknown` literal → `t('unknown')`.

- [ ] **Step 3: Build + smoke**

Run: `cd frontend && npm run lint && npm run build`. Smoke zh: diagnostics section fully translated.

- [ ] **Step 4: Commit (if user permits)**

```bash
git add frontend/app/diagnostics-panel.tsx frontend/messages/en.json frontend/messages/zh.json
git commit -m "feat(frontend): translate diagnostics panel"
```

---

## Task 11: Translate runtime sections (RuntimeStatusPanel, TenantSettings, dashboard-runtime notifications)

**Files:**

- Modify: `frontend/app/dashboard-runtime-sections.tsx`
- Modify: `frontend/app/dashboard-runtime.tsx`
- Modify: `frontend/messages/en.json`, `frontend/messages/zh.json`

- [ ] **Step 1: Add `tenantSettings` namespace**

`messages/en.json` add:

```json
"tenantSettings": {
  "title": "Tenant settings",
  "subtitleTenant": "{name} operational references",
  "subtitleNone": "No tenant selected",
  "meta": "No token values shown",
  "groupTenant": "Tenant",
  "id": "ID",
  "slug": "Slug",
  "created": "Created",
  "groupAuth": "Authentication",
  "source": "Source",
  "provider": "Provider",
  "cookieName": "Cookie name",
  "secretValues": "Secret values",
  "hidden": "Hidden",
  "groupOps": "Operations",
  "diagnosticsValue": "See the Diagnostics section",
  "developerRef": "Developer reference",
  "agentPairing": "Agent pairing",
  "apiTokens": "API tokens",
  "linkedAgents": "Linked agents",
  "noLinkedAgents": "No linked agents",
  "printerCompat": "Printer compatibility",
  "noPrinters": "No reported printers",
  "runDiagnostics": "Run diagnostics from the Diagnostics section",
  "tenant": "Tenant",
  "webSocket": "WebSocket",
  "lastEvent": "Last event",
  "auth": "Auth",
  "authValue": "{label} · cookie {cookie}",
  "noTenant": "No tenant",
  "notifications": "Notifications",
  "noNotifications": "No live notifications",
  "liveNotificationsAria": "Live notifications"
}
```

`messages/zh.json` add:

```json
"tenantSettings": {
  "title": "租户设置",
  "subtitleTenant": "{name} 运维参考",
  "subtitleNone": "未选择租户",
  "meta": "不显示令牌值",
  "groupTenant": "租户",
  "id": "ID",
  "slug": "Slug",
  "created": "创建时间",
  "groupAuth": "身份认证",
  "source": "来源",
  "provider": "提供商",
  "cookieName": "Cookie 名称",
  "secretValues": "密钥值",
  "hidden": "已隐藏",
  "groupOps": "运维",
  "diagnosticsValue": "见“诊断”分区",
  "developerRef": "开发者参考",
  "agentPairing": "Agent 配对",
  "apiTokens": "API 令牌",
  "linkedAgents": "已连接 Agent",
  "noLinkedAgents": "无已连接 Agent",
  "printerCompat": "打印机兼容性",
  "noPrinters": "暂无上报打印机",
  "runDiagnostics": "请在“诊断”分区运行诊断",
  "tenant": "租户",
  "webSocket": "WebSocket",
  "lastEvent": "最近事件",
  "auth": "认证",
  "authValue": "{label} · Cookie {cookie}",
  "noTenant": "无租户",
  "notifications": "通知",
  "noNotifications": "暂无实时通知",
  "liveNotificationsAria": "实时通知"
}
```

- [ ] **Step 2: Wire `dashboard-runtime-sections.tsx`**

`const t = useTranslations('tenantSettings')`, `const tLive = useTranslations('runtime.liveState')`, `const format = useFormatter()` + bound `formatDate`. Replace literals; `formatLiveState(liveState, tLive)`; `<FormattedDate value={...} />` for dates; `DetailLine`/`DetailGroup` `title`/`label` props → translated strings. The developer-reference paths stay literal (they're API paths). `RuntimeField` `label` props translated. `aria-label="Live notifications"` → `t('liveNotificationsAria')`.

- [ ] **Step 3: Wire notification + error/action strings in `dashboard-runtime.tsx`**

In the `useEffect` that builds notifications, the `title`/`detail` strings are currently English. These run inside a client component effect. Add module-scoped access to translations: since this is inside `useEffect` (not render), `useTranslations` can't be called there. **Refactor:** move the notification _text_ out of the effect by storing locale-neutral keys instead of English, and translate at render in `RuntimeStatusPanel`. Change `RuntimeNotification` to carry `titleKey`/`detailKey` (namespace+key+values) instead of `title`/`detail`. Then `RuntimeStatusPanel` (a client component) resolves them via the same `useResolvedText` hook pattern from Task 6 (hoist that hook to a shared `components/use-resolved-text.ts`).

Concretely:

- Create `frontend/components/use-resolved-text.ts`:

```ts
"use client";
import { useTranslations } from "next-intl";
type TextKey = {
  namespace: string;
  key: string;
  values?: Record<string, string | number>;
};
export function useResolvedText() {
  return (k: TextKey) => {
    const t = useTranslations(k.namespace);
    return t(k.key, k.values);
  };
}
```

> Note: calling `useTranslations` inside the returned function violates rules-of-hooks (it's a hook). **Corrected:** make `useResolvedText` accept the key and call the hook at top level:

```ts
"use client";
import { useTranslations } from "next-intl";
type TextKey = {
  namespace: string;
  key: string;
  values?: Record<string, string | number>;
};
export function useResolvedText(k: TextKey): string {
  const t = useTranslations(k.namespace);
  return t(k.key, k.values);
}
```

Each notification row calls `useResolvedText(notification.titleKey)` — but hooks can't be called in a `.map`. **Final corrected approach:** resolve notification text by rendering a small `<NotificationRow>` child component (one hook call per row). Create `components/notification-row.tsx` that takes a `RuntimeNotification` and calls `useResolvedText` for title/detail. Move `RuntimeNotification` rendering into it. Apply this pattern. Update `RuntimeNotification` type in `dashboard-runtime-helpers.ts` to `{ key, titleKey, detailKey, timestamp }`.

In `dashboard-runtime.tsx` effect, replace the English `title`/`detail` literals with `titleKey`/`detailKey` objects referencing `runtime.notification.*`. For dynamic details (`${printer.name} (${printer.serial_number})`, `${job.artifact.filename} dispatch ${job.status}`, `${formatJobRecoveryState(job)}`), store the values in `detailKey.values` and use message interpolation. The recovery-state string inside a notification is itself translated — for those, store a nested key reference: simpler is to store `jobId` and let `NotificationRow` call `formatJobRecoveryState(job,...)`. Since the effect doesn't have `job` easily, store the raw fields in values and translate with a dedicated message. Use `runtime.notification.jobDispatchDetail` = `"{filename} dispatch {status}"` (en) / `"{filename} 派发 {status}"` (zh). Add these keys to `runtime.notification`:

`messages/en.json` add under `runtime.notification`:

```json
"printerDetail": "{name} ({serial})",
"jobDispatchDetail": "{filename} dispatch {status}",
"jobErrorFallback": "{filename}"
```

zh:

```json
"printerDetail": "{name} ({serial})",
"jobDispatchDetail": "{filename} 派发 {status}",
"jobErrorFallback": "{filename}"
```

Replace `ACTION_STATUS_MESSAGES` usage: `formatActionStatus` now reads `runtime.actionStatus.*`. Add a translator-scoped lookup: in `dashboard-runtime.tsx` (component body) `const tStatus = useTranslations('runtime.actionStatus')` and rewrite `formatActionStatus(status)` to `formatActionStatus(status, tStatus)` where the helper checks `tStatus.has(status)` then returns `tStatus(status)`, else falls back to the capitalize logic. Update `formatActionStatus` signature accordingly (in `dashboard-runtime.tsx` it's a local function). The `errors` block: `Hub data is incomplete.` → `{tErr('errorsIncomplete')}` with `const tErr = useTranslations('runtime.notification')`, joining `errors` after.

- [ ] **Step 4: Build + smoke**

Run: `cd frontend && npm run lint && npm run build`. Smoke zh: tenant settings, runtime status panel, and the notification list render Chinese; live notifications that fire (e.g. disconnect) show translated text.

- [ ] **Step 5: Commit (if user permits)**

```bash
git add frontend/app/dashboard-runtime-sections.tsx frontend/app/dashboard-runtime.tsx frontend/app/dashboard-runtime-helpers.tsx frontend/components/use-resolved-text.ts frontend/components/notification-row.tsx frontend/messages/en.json frontend/messages/zh.json
git commit -m "feat(frontend): translate runtime status, tenant settings, and notifications"
```

---

## Task 12: Translate admin panel

**Files:**

- Modify: `frontend/app/admin-panel.tsx`
- Modify: `frontend/messages/en.json`, `frontend/messages/zh.json`

- [ ] **Step 1: Add `admin` namespace**

`messages/en.json` add:

```json
"admin": {
  "title": "Tenant administration",
  "subtitleNone": "No tenant selected",
  "subtitleUnavailable": "{name} admin data is unavailable",
  "metaAdmin": "Admin",
  "metaRestricted": "Restricted",
  "metaSecrets": "Secrets are not stored",
  "noTenantTitle": "No tenant selected",
  "noTenantMessage": "Select a tenant to manage users, tokens, and agent pairings.",
  "unavailableTitle": "Admin data unavailable",
  "unavailableMessage": "The current auth context cannot read tenant admin resources.",
  "subtitleTenant": "{name} users, tokens, and audit trail",
  "createJoinLink": "Create join link",
  "role": "Role",
  "verifiedEmail": "Verified email",
  "ttlSeconds": "TTL seconds",
  "maxUses": "Max uses",
  "creating": "Creating...",
  "createLink": "Create link",
  "createTenantToken": "Create tenant token",
  "name": "Name",
  "scopes": "Scopes",
  "expiresAt": "Expires at",
  "createToken": "Create token",
  "pairAgent": "Pair agent",
  "agentName": "Agent name",
  "createPairing": "Create pairing",
  "users": "Users",
  "usersMeta": "{count} users",
  "noUsersTitle": "No users",
  "noUsersMessage": "Create a tenant user to assign operator or viewer access.",
  "colUser": "User",
  "colRole": "Role",
  "colIdentities": "Identities",
  "colUpdate": "Update",
  "save": "Save",
  "joinLinks": "Join links",
  "joinLinksMeta": "{count} links",
  "noJoinLinksTitle": "No join links",
  "noJoinLinksMessage": "Create a join link to invite externally authenticated users.",
  "usedRatio": "{used}/{max} used",
  "revoked": "Revoked",
  "emailConstraint": "Email {email}",
  "anyVerifiedEmail": "Any verified email",
  "expires": "Expires {date}",
  "revoke": "Revoke",
  "revokeJoinTitle": "Revoke join link",
  "revokeJoinMessage": "Revoke this join link? It will no longer accept new members; existing members keep their access.",
  "revokeJoinConfirm": "Revoke link",
  "tenantTokens": "Tenant tokens",
  "tenantTokensMeta": "{count} tokens",
  "noTokensTitle": "No tenant tokens",
  "noTokensMessage": "Create scoped tenant tokens for automation or plugin login.",
  "expiresNever": "Expires never",
  "rotating": "Rotating...",
  "rotate": "Rotate",
  "revokeTokenTitle": "Revoke tenant token",
  "revokeTokenMessage": "Revoke this tenant token? Anything using it (automation, agents, plugins) will stop authenticating immediately.",
  "revokeTokenConfirm": "Revoke token",
  "rotateTokenTitle": "Rotate tenant token",
  "rotateTokenMessage": "Rotate this tenant token? The current secret stops working immediately — update anything using it (automation, agents, plugins) with the new value.",
  "rotateTokenConfirm": "Rotate token",
  "tokenShownOnce": "This token is shown once and is not persisted by the browser.",
  "joinTokenShownOnce": "This join token is shown once and is not persisted by the browser.",
  "pairingShownOnce": "This pairing output is shown once and is not persisted by the browser.",
  "agents": "Agents",
  "agentsMeta": "{count} linked",
  "noLinkedAgents": "No linked agents",
  "auditEvents": "Audit events",
  "auditMeta": "{count} recent",
  "noAuditEvents": "No audit events",
  "idLabel": "ID"
}
```

`messages/zh.json` add:

```json
"admin": {
  "title": "租户管理",
  "subtitleNone": "未选择租户",
  "subtitleUnavailable": "{name} 管理数据不可用",
  "metaAdmin": "管理",
  "metaRestricted": "受限",
  "metaSecrets": "不存储密钥",
  "noTenantTitle": "未选择租户",
  "noTenantMessage": "请选择一个租户以管理用户、令牌和 Agent 配对。",
  "unavailableTitle": "管理数据不可用",
  "unavailableMessage": "当前认证上下文无法读取租户管理资源。",
  "subtitleTenant": "{name} 的用户、令牌与审计记录",
  "createJoinLink": "创建加入链接",
  "role": "角色",
  "verifiedEmail": "已验证邮箱",
  "ttlSeconds": "有效期秒数",
  "maxUses": "最大使用次数",
  "creating": "创建中...",
  "createLink": "创建链接",
  "createTenantToken": "创建租户令牌",
  "name": "名称",
  "scopes": "范围",
  "expiresAt": "过期时间",
  "createToken": "创建令牌",
  "pairAgent": "配对 Agent",
  "agentName": "Agent 名称",
  "createPairing": "创建配对",
  "users": "用户",
  "usersMeta": "{count} 个用户",
  "noUsersTitle": "无用户",
  "noUsersMessage": "创建租户用户以授予操作员或查看者权限。",
  "colUser": "用户",
  "colRole": "角色",
  "colIdentities": "身份",
  "colUpdate": "更新",
  "save": "保存",
  "joinLinks": "加入链接",
  "joinLinksMeta": "{count} 个链接",
  "noJoinLinksTitle": "无加入链接",
  "noJoinLinksMessage": "创建加入链接以邀请外部认证的用户。",
  "usedRatio": "已用 {used}/{max}",
  "revoked": "已吊销",
  "emailConstraint": "邮箱 {email}",
  "anyVerifiedEmail": "任意已验证邮箱",
  "expires": "过期 {date}",
  "revoke": "吊销",
  "revokeJoinTitle": "吊销加入链接",
  "revokeJoinMessage": "吊销此加入链接？它将不再接受新成员；现有成员的权限不受影响。",
  "revokeJoinConfirm": "吊销链接",
  "tenantTokens": "租户令牌",
  "tenantTokensMeta": "{count} 个令牌",
  "noTokensTitle": "无租户令牌",
  "noTokensMessage": "为自动化或插件登录创建带范围的租户令牌。",
  "expiresNever": "永不过期",
  "rotating": "轮换中...",
  "rotate": "轮换",
  "revokeTokenTitle": "吊销租户令牌",
  "revokeTokenMessage": "吊销此租户令牌？使用它的对象（自动化、Agent、插件）将立即停止认证。",
  "revokeTokenConfirm": "吊销令牌",
  "rotateTokenTitle": "轮换租户令牌",
  "rotateTokenMessage": "轮换此租户令牌？当前密钥立即失效——请用新值更新所有使用它的对象（自动化、Agent、插件）。",
  "rotateTokenConfirm": "轮换令牌",
  "tokenShownOnce": "此令牌仅显示一次，浏览器不会保存。",
  "joinTokenShownOnce": "此加入令牌仅显示一次，浏览器不会保存。",
  "pairingShownOnce": "此配对输出仅显示一次，浏览器不会保存。",
  "agents": "Agent",
  "agentsMeta": "已连接 {count} 个",
  "noLinkedAgents": "无已连接 Agent",
  "auditEvents": "审计事件",
  "auditMeta": "近期 {count} 条",
  "noAuditEvents": "无审计事件",
  "idLabel": "ID"
}
```

- [ ] **Step 2: Wire `admin-panel.tsx`**

`const t = useTranslations('admin')`, `const format = useFormatter()` + bound `formatDate`, `import { FormattedDate } from '../components/formatted-date'`. Replace every literal with its key. Note:

- `roles` array values (`tenant_admin`, `operator`, `viewer`) stay as-is (they're role identifiers rendered in `<Tag>` and `<option>`; they pass through `prettifyToken`/token translator — add them to `tokens` namespace if user-facing display should translate, otherwise leave English). **Decision:** add role keys to `tokens`: `tenant_admin`/`operator`/`viewer` → en/zh, and pass the token translator into `Tag` (already done in Task 6 Step 5 for `Tag`). Add to both messages under `tokens`:
  - en: `"tenant_admin": "Tenant admin", "operator": "Operator", "viewer": "Viewer"`
  - zh: `"tenant_admin": "租户管理员", "operator": "操作员", "viewer": "查看者"`
- `formatDate(...)` JSX → `<FormattedDate value={...} />`. For `Expires {formatDate(...)}` and `Expires never`, use `t('expires', { date })` where `date` is pre-formatted via the bound `formatDate` (since `t()` returns a string, embedding a formatted date string in values is fine).
- `SecretActionResult` `state.message` comes from the server (action result) — leave as-is (server-provided English). Translate only the static `"This token is shown once..."` sentences.

- [ ] **Step 3: Build + smoke**

Run: `cd frontend && npm run lint && npm run build`. Smoke zh: admin panel fully translated; role tags translate; dates localized.

- [ ] **Step 4: Commit (if user permits)**

```bash
git add frontend/app/admin-panel.tsx frontend/messages/en.json frontend/messages/zh.json
git commit -m "feat(frontend): translate tenant admin panel"
```

---

## Task 13: Translate standalone pages (onboarding, sign-in, join)

**Files:**

- Modify: `frontend/app/onboarding-panel.tsx`
- Modify: `frontend/app/plugin-sign-in/page.tsx`
- Modify: `frontend/app/plugin-sign-in/plugin-ticket-form.tsx`
- Modify: `frontend/app/join/page.tsx`
- Modify: `frontend/app/join/token-form.tsx`
- Modify: `frontend/messages/en.json`, `frontend/messages/zh.json`

- [ ] **Step 1: Add `onboarding`, `signIn`, `join` namespaces**

`messages/en.json` add:

```json
"onboarding": {
  "title": "Tenant onboarding",
  "subtitle": "{name} authenticated by {provider}",
  "noEmail": "No verified email",
  "verifiedTitle": "Verified email required",
  "verifiedMessage": "Verify the email address in the external auth provider before creating or joining a tenant.",
  "createTitle": "Create tenant",
  "createMessage": "The current external identity becomes tenant admin.",
  "tenantName": "Tenant name",
  "tenantSlug": "Tenant slug",
  "createSubmit": "Create tenant",
  "joinTitle": "Join tenant",
  "joinMessage": "Open an invitation URL from a tenant admin, or paste the token on the join page.",
  "openJoin": "Open join page",
  "signIn": "Sign in",
  "signOut": "Sign out"
},
"signIn": {
  "title": "Studio sign-in",
  "subtitle": "Create a one-use Pandar plugin login ticket",
  "meta": "Plugin",
  "externalUnavailableTitle": "External auth unavailable",
  "externalConfigMessage": "Configure external auth before creating plugin login tickets.",
  "authUnavailableTitle": "Auth token unavailable",
  "authMessage": "Authenticate before creating plugin login tickets.",
  "tenantLookupTitle": "Tenant lookup unavailable",
  "noTenantsTitle": "No tenants available",
  "noTenantsMessage": "Authenticate with a tenant-scoped account before signing in from Studio.",
  "selectTenant": "Select tenant",
  "tenant": "Tenant",
  "continue": "Continue",
  "callbackUrl": "Local callback URL",
  "signInSubmit": "Sign in to Studio"
},
"join": {
  "title": "Join tenant",
  "subtitle": "Accept an invitation with {provider} authentication",
  "joinToken": "Join token",
  "joinSubmit": "Join tenant",
  "signIn": "Sign in",
  "signOut": "Sign out"
}
```

`messages/zh.json` add:

```json
"onboarding": {
  "title": "租户开通",
  "subtitle": "{name} 由 {provider} 认证",
  "noEmail": "无已验证邮箱",
  "verifiedTitle": "需要已验证邮箱",
  "verifiedMessage": "请在创建或加入租户前，先在外部认证提供商中验证邮箱地址。",
  "createTitle": "创建租户",
  "createMessage": "当前外部身份将成为租户管理员。",
  "tenantName": "租户名称",
  "tenantSlug": "租户 Slug",
  "createSubmit": "创建租户",
  "joinTitle": "加入租户",
  "joinMessage": "打开租户管理员提供的邀请 URL，或在加入页面粘贴令牌。",
  "openJoin": "打开加入页面",
  "signIn": "登录",
  "signOut": "退出登录"
},
"signIn": {
  "title": "Studio 登录",
  "subtitle": "创建一次性的 Pandar 插件登录票据",
  "meta": "插件",
  "externalUnavailableTitle": "外部认证不可用",
  "externalConfigMessage": "创建插件登录票据前请先配置外部认证。",
  "authUnavailableTitle": "认证令牌不可用",
  "authMessage": "创建插件登录票据前请先完成认证。",
  "tenantLookupTitle": "租户查询不可用",
  "noTenantsTitle": "无可用租户",
  "noTenantsMessage": "从 Studio 登录前，请使用具备租户权限的账号进行认证。",
  "selectTenant": "选择租户",
  "tenant": "租户",
  "continue": "继续",
  "callbackUrl": "本地回调 URL",
  "signInSubmit": "登录到 Studio"
},
"join": {
  "title": "加入租户",
  "subtitle": "使用 {provider} 认证接受邀请",
  "joinToken": "加入令牌",
  "joinSubmit": "加入租户",
  "signIn": "登录",
  "signOut": "退出登录"
}
```

- [ ] **Step 2: Wire `onboarding-panel.tsx`**

`const t = useTranslations('onboarding')`. Replace literals (title, subtitle with `{name}`/`{provider}` interpolation, `'No verified email'`, EmptyState title/message, "Create tenant"/"Join tenant" blocks, Input labels, button labels, ProviderLinks `"Sign in"`/`"Sign out"`). `me.identity.email ?? 'No verified email'` → `me.identity.email ?? t('noEmail')`.

- [ ] **Step 3: Wire `plugin-sign-in/page.tsx`**

This is a server component. Use `import { useTranslations } from 'next-intl'` (works synchronously in RSC) — `const t = useTranslations('signIn')`. Replace SectionHeader title/subtitle/meta, all EmptyState title/message literals, `"Select tenant"`, `"Tenant"`, `"Continue"`. The dynamic `readiness.error` / `tenantResult.error` strings come from local fetch helpers that build English messages (e.g. `` `Tenant lookup returned ${response.status}` ``) — **leave those server-built error strings English** (they're diagnostic, not primary UI; translating them would require threading translators into fetch helpers, out of scope per YAGNI). They render inside `EmptyState message={...}` which is fine.

- [ ] **Step 4: Wire `plugin-sign-in/plugin-ticket-form.tsx`**

`'use client'`. `const t = useTranslations('signIn')`. Replace `"Local callback URL"` → `t('callbackUrl')`, `"Sign in to Studio"` → `t('signInSubmit')`.

- [ ] **Step 5: Wire `join/page.tsx`**

Server component. `const t = useTranslations('join')`. SectionHeader title/subtitle (`{provider}` interpolation). ProviderLinks literals via the same keys.

- [ ] **Step 6: Wire `join/token-form.tsx`**

`'use client'`. `const t = useTranslations('join')`. `"Join token"` → `t('joinToken')`, `"Join tenant"` → `t('joinSubmit')`.

- [ ] **Step 7: Build + smoke**

Run: `cd frontend && npm run lint && npm run build`. Smoke zh: visit `/sign-in`, `/join`, `/onboarding`-equivalent (onboarding shows when me has no tenants) — all render Chinese; the `LanguageSwitcher` in `SectionHeader` toggles them too. The existing `app/[locale]/sign-in` route still resolves (e.g. `/en/sign-in`) and renders translated content via the cookie.

- [ ] **Step 8: Commit (if user permits)**

```bash
git add frontend/app/onboarding-panel.tsx frontend/app/plugin-sign-in frontend/app/join frontend/messages/en.json frontend/messages/zh.json
git commit -m "feat(frontend): translate onboarding, sign-in, and join pages"
```

---

## Task 14: Final verification, cleanup, roadmap update

**Files:**

- Verify: all modified files.
- Modify: `docs/roadmap.md` (project root).

- [ ] **Step 1: Lint + build clean**

Run: `cd frontend && npm run lint && npm run build`
Expected: both succeed with no warnings about missing keys.

- [ ] **Step 2: Grep for leftover English literals in JSX**

Run from `frontend/`:

```bash
rg -n ">[A-Z][a-zA-Z ]{3,}<|placeholder=\"[A-Z]|aria-label=\"[A-Z][a-z]" app --glob '*.tsx'
```

Expected: every remaining match is either a deliberately-untranslated diagnostic string (fetch-helper error builders in `plugin-sign-in/page.tsx`, `SecretActionResult.state.message`) or a `font-mono` identifier. If a real UI string slipped through, add it to the right namespace and replace. Iterate until clean.

- [ ] **Step 3: Verify key parity between locales**

Run:

```bash
node -e "const en=require('./messages/en.json'),zh=require('./messages/zh.json'); function k(o,p){p=p||[];return Object.entries(o).flatMap(([kk,v])=>v&&typeof v==='object'?k(v,p.concat(kk)):[p.concat(kk).join('.')])} const a=new Set(k(en)),b=new Set(k(zh)); console.log('en-only',[...a].filter(x=>!b.has(x))); console.log('zh-only',[...b].filter(x=>!a.has(x)))"
```

(workdir `frontend/`). Expected: both lists empty.

- [ ] **Step 4: Manual end-to-end smoke**

- `npm run dev`. With a clear browser profile (no `locale` cookie):
  - `curl -H 'Accept-Language: zh' http://localhost:3000/ | grep 'html lang'` → `zh`.
  - `curl -H 'Accept-Language: en' http://localhost:3000/ | grep 'html lang'` → `en`.
- In browser, toggle the switcher in the dashboard header: page re-renders in the chosen locale without full reload; cookie `locale` set; reload preserves it.
- Toggle from a standalone page (`/sign-in`): same behavior; server-rendered HTML arrives already translated.
- Failed-job attention row actions ("Reprint"/"Retry dispatch") still dispatch correctly with `locale=zh` (proves the `reason`-based refactor).
- `app/[locale]/sign-in` (e.g. `/en/sign-in`) still loads (Studio compat intact).

- [ ] **Step 5: Update `docs/roadmap.md`**

Append an entry under the most recent phase's "Completed" / progress list (follow the file's existing voice):

```
- Added frontend localization (中文 / English) via next-intl in cookie-based non-segment mode; locale resolved from the `locale` cookie with Accept-Language negotiation and mirrored in the zustand `pandar.settings` store. Translated all user-facing strings across dashboard, dispatch, recovery, diagnostics, runtime, admin, and the standalone onboarding/sign-in/join pages; dates and numbers localized; machine-status tokens translated with prettify fallback. Language switcher placed in the dashboard header and standalone page section headers. The existing `[locale]/sign-in` Bambu Studio WebView alias is preserved.
```

- [ ] **Step 6: Commit (if user permits)**

```bash
git add docs/roadmap.md
git commit -m "docs(roadmap): note frontend localization"
```

---

## Self-Review Notes

- **Spec coverage:** every section of `docs/superpowers/specs/2026-06-28-frontend-localization-design.md` maps to a task: locale resolution (Task 1), cookie+zustand+switcher (Task 2), `<FormattedDate>`/`formatBytes` (Task 3), dynamic-builder refactor + ICU plurals + `tokens` namespace (Task 4), header/nav/switcher placement (Task 5), overview/status/attention (Task 6), inventory (Task 7), dispatch (Task 8), recovery (Task 9), diagnostics (Task 10), runtime/tenant-settings/notifications + `generateMetadata` (Task 1 + Task 11), admin (Task 12), standalone pages (Task 13), verification + roadmap (Task 14).
- **Type consistency:** `Translator` type alias defined once in `dashboard-runtime-helpers.ts` and reused. `AttentionItem.reason` values match the `reason === 'job_print_failed'`/`'job_dispatch_failed'` branches in `AttentionAction`. `RuntimeNotification` changed to `{ key, titleKey, detailKey, timestamp }` consistently in helper type, effect producer, and `NotificationRow` consumer.
- **Risk flagged inline:** the Task 4 build-green checkpoint uses default translators that reproduce prior English output, so the tree compiles after Task 4 even before callers migrate. Each subsequent task replaces one helper's call sites with a real translator. Task 4 must NOT leave half-migrated call sites.
