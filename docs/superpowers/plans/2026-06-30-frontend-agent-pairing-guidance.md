# Frontend Agent Pairing Guidance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add tenant-aware agent pairing guidance to the Agents page so tenant administrators can create pairings in context and restricted users understand who can pair agents.

**Architecture:** Add one focused client component for the Agents-page guidance, render it above linked agents, and reuse the existing pairing server-action form without changing backend behavior. New guidance copy lives under a new top-level `agentPairing` message namespace; the reused form and one-time secret result continue using existing `admin` messages.

**Tech Stack:** Next.js 16, React 19, next-intl, Tailwind CSS, Vitest, Testing Library.

## Global Constraints

- Pairing creation is limited to existing backend authorization; no backend/API/schema changes for this scope.
- Product UI must remain calm, technical, trustworthy, dense, scannable, and restrained.
- Do not add persistence or use zustand for this change.
- Reuse `CreateAgentPairingForm` and `SecretActionResult`; do not refactor shared admin components only to rename message keys.
- New guidance-section copy must be localized in `frontend/messages/en.json` and `frontend/messages/zh.json` under a top-level `agentPairing` namespace.
- The no-tenant and restricted states must not render the pairing form.
- Update `docs/roadmap.md` after implementation.

---

## File Structure

- Create `frontend/app/agent-pairing-guidance.tsx`: client component that renders the guidance section, tenant/no-tenant/restricted states, setup steps, and existing pairing form.
- Create `frontend/app/agent-pairing-guidance.test.tsx`: focused React tests using `NextIntlClientProvider` and English messages.
- Modify `frontend/app/dashboard-view-content.tsx`: import and render the guidance section at the top of `AgentsView`, passing `selectedTenant` and `adminUnavailable`.
- Modify `frontend/messages/en.json`: add top-level `agentPairing` messages.
- Modify `frontend/messages/zh.json`: add matching top-level `agentPairing` messages.
- Modify `docs/roadmap.md`: record the completed Agents pairing guidance UI and next step.

### Task 1: Agents Page Pairing Guidance

**Files:**

- Create: `frontend/app/agent-pairing-guidance.tsx`
- Create: `frontend/app/agent-pairing-guidance.test.tsx`
- Modify: `frontend/app/dashboard-view-content.tsx`
- Modify: `frontend/messages/en.json`
- Modify: `frontend/messages/zh.json`
- Modify: `docs/roadmap.md`

**Interfaces:**

- Consumes: `CreateAgentPairingForm({ tenantId }: { tenantId: string })` from `frontend/app/admin-settings-panel.tsx`.
- Consumes: `Tenant` type from `frontend/app/dashboard-types.ts`.
- Produces: `AgentPairingGuidance({ selectedTenant, restricted }: { selectedTenant: Tenant | null; restricted: boolean })` from `frontend/app/agent-pairing-guidance.tsx`.
- Produces: `agentPairing` message namespace with keys `title`, `subtitleTenant`, `subtitleNone`, `subtitleRestricted`, `summary`, `stepsTitle`, `stepSelectTenant`, `stepCreate`, `stepCopy`, `stepStart`, `restrictedTitle`, `restrictedDetail`, `noTenantTitle`, and `noTenantDetail`.

- [ ] **Step 1: Write the failing test**

Create `frontend/app/agent-pairing-guidance.test.tsx` with this content:

```tsx
import type { ReactNode } from "react";
import { NextIntlClientProvider } from "next-intl";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import {
  DashboardViewContent,
  type DashboardViewContentProps,
} from "./dashboard-view-content";
import type { Tenant } from "./dashboard-types";

vi.mock("./actions", () => ({
  controlPrinter: vi.fn(),
  createAgentPairing: vi.fn(),
  createJoinLink: vi.fn(),
  createTenantFromExternal: vi.fn(),
  createTenantToken: vi.fn(),
  diagnosePrinter: vi.fn(),
  discoverPrinters: vi.fn(),
  duplicateJob: vi.fn(),
  refreshAllAgents: vi.fn(),
  refreshPrinters: vi.fn(),
  retryDispatchJob: vi.fn(),
  retryDispatchJobs: vi.fn(),
  reprintJob: vi.fn(),
  revokeJoinLink: vi.fn(),
  revokeTenantToken: vi.fn(),
  rotateTenantToken: vi.fn(),
  updateTenantUserRole: vi.fn(),
}));

function renderWithMessages(children: ReactNode) {
  return render(
    <NextIntlClientProvider locale="en" messages={en}>
      {children}
    </NextIntlClientProvider>,
  );
}

const tenant: Tenant = {
  id: "tenant-1",
  slug: "factory",
  display_name: "Factory Floor",
  created_at: "2026-06-30T00:00:00Z",
};

const baseProps: DashboardViewContentProps = {
  view: "agents",
  auth: {
    source: "none",
    cookieName: "pandar_session",
    provider: "none",
    signInUrl: null,
    signOutUrl: null,
  },
  selectedTenant: tenant,
  health: {
    printersTotal: 0,
    printersOnline: 0,
    agentsTotal: 0,
    agentsConnected: 0,
    jobsActive: 0,
    jobsFailed: 0,
  },
  attentionItems: [],
  topSeverity: null,
  liveState: "idle",
  lastEventAt: null,
  fleetEmpty: true,
  printers: [],
  agents: [],
  jobs: [],
  selectedCommand: null,
  commandData: null,
  notifications: [],
  users: [],
  userIdentities: [],
  tenantTokens: [],
  joinLinks: [],
  auditEvents: [],
  adminUnavailable: false,
};

function renderAgentsView(overrides: Partial<DashboardViewContentProps> = {}) {
  return renderWithMessages(
    <DashboardViewContent {...{ ...baseProps, ...overrides }} />,
  );
}

describe("Agents view pairing guidance", () => {
  it("renders guidance above linked agents and shows the pairing form for an available tenant admin context", () => {
    renderAgentsView();

    const pairingHeading = screen.getByRole("heading", {
      name: "Pair a local agent",
    });
    const linkedAgentsHeading = screen.getByRole("heading", {
      name: "Linked agents",
    });

    expect(pairingHeading).toBeVisible();
    expect(
      pairingHeading.compareDocumentPosition(linkedAgentsHeading) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      screen.getByText("Create a pairing for Factory Floor."),
    ).toBeVisible();
    expect(
      screen.getByText(/Copy the generated environment block/),
    ).toBeVisible();
    expect(screen.getByText(/Start or restart pandar-agent/)).toBeVisible();
    expect(screen.getByLabelText("Agent name")).toHaveAttribute("name", "name");
    expect(
      screen.getByRole("button", { name: "Create pairing" }),
    ).toHaveAttribute("type", "submit");
  });

  it("does not render the pairing form when no tenant is selected", () => {
    renderAgentsView({ selectedTenant: null });

    expect(
      screen.getByRole("heading", { name: "Pair a local agent" }),
    ).toBeVisible();
    expect(
      screen.getByText("Select a tenant before creating an agent pairing."),
    ).toBeVisible();
    expect(
      screen.getByText(
        "Choose a tenant from the header to enable pairing creation.",
      ),
    ).toBeVisible();
    expect(screen.queryByLabelText("Agent name")).not.toBeInTheDocument();
  });

  it("does not render the pairing form when admin resources are restricted", () => {
    renderAgentsView({ adminUnavailable: true });

    expect(
      screen.getByText(
        "Only a tenant administrator or agent-registration-capable principal can create pairings.",
      ),
    ).toBeVisible();
    expect(
      screen.getByText(
        "Use a tenant admin account or scoped registration token before creating this pairing.",
      ),
    ).toBeVisible();
    expect(screen.queryByLabelText("Agent name")).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
npm --prefix frontend test -- agent-pairing-guidance.test.tsx
```

Expected: FAIL because the Agents view does not render the `Pair a local agent` guidance heading yet.

- [ ] **Step 3: Add localized guidance messages**

Modify `frontend/messages/en.json` by adding this top-level object after the existing `diagnostics` object and before `admin`:

```json
  "agentPairing": {
    "title": "Pair a local agent",
    "subtitleTenant": "Create a one-time pairing for {name} ({slug}).",
    "subtitleNone": "Select a tenant before creating an agent pairing.",
    "subtitleRestricted": "Only a tenant administrator or agent-registration-capable principal can create pairings.",
    "summary": "Pairing produces an environment block for the machine running pandar-agent. The output is shown once, so copy it before leaving this page.",
    "stepsTitle": "Setup flow",
    "stepSelectTenant": "Select a tenant, then create a pairing.",
    "stepCreate": "Create a pairing for {name}.",
    "stepCopy": "Copy the generated environment block into the machine running pandar-agent.",
    "stepStart": "Start or restart pandar-agent, then run discovery after it appears online.",
    "restrictedTitle": "Pairing requires admin access",
    "restrictedDetail": "Use a tenant admin account or scoped registration token before creating this pairing.",
    "noTenantTitle": "Tenant required",
    "noTenantDetail": "Choose a tenant from the header to enable pairing creation."
  },
```

Modify `frontend/messages/zh.json` by adding the matching top-level object after the existing `diagnostics` object and before `admin`:

```json
  "agentPairing": {
    "title": "配对本地 Agent",
    "subtitleTenant": "为 {name} ({slug}) 创建一次性配对。",
    "subtitleNone": "请先选择租户，再创建 Agent 配对。",
    "subtitleRestricted": "只有租户管理员或具备 Agent 注册权限的主体可以创建配对。",
    "summary": "配对会为运行 pandar-agent 的机器生成环境变量块。输出只显示一次，离开此页面前请先复制。",
    "stepsTitle": "设置流程",
    "stepSelectTenant": "先选择租户，然后创建配对。",
    "stepCreate": "为 {name} 创建配对。",
    "stepCopy": "将生成的环境变量块复制到运行 pandar-agent 的机器。",
    "stepStart": "启动或重启 pandar-agent，待其上线后再运行发现。",
    "restrictedTitle": "配对需要管理员权限",
    "restrictedDetail": "请使用租户管理员账号或具备注册范围的令牌后再创建此配对。",
    "noTenantTitle": "需要选择租户",
    "noTenantDetail": "请从页头选择租户以启用配对创建。"
  },
```

- [ ] **Step 4: Implement the guidance component**

Create `frontend/app/agent-pairing-guidance.tsx` with this content:

```tsx
"use client";

import { useTranslations } from "next-intl";

import { CreateAgentPairingForm } from "./admin-settings-panel";
import type { Tenant } from "./dashboard-types";

export function AgentPairingGuidance({
  selectedTenant,
  restricted,
}: {
  selectedTenant: Tenant | null;
  restricted: boolean;
}) {
  const t = useTranslations("agentPairing");

  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <div className="grid gap-0 lg:grid-cols-[minmax(0,1fr)_minmax(320px,0.72fr)]">
        <div className="border-b border-slate-200 px-4 py-4 lg:border-b-0 lg:border-r">
          <div>
            <h2 className="text-base font-semibold text-slate-950">
              {t("title")}
            </h2>
            <p className="mt-0.5 text-sm text-slate-600">
              {selectedTenant
                ? restricted
                  ? t("subtitleRestricted")
                  : t("subtitleTenant", {
                      name: selectedTenant.display_name,
                      slug: selectedTenant.slug,
                    })
                : t("subtitleNone")}
            </p>
          </div>
          <p className="mt-3 max-w-3xl text-sm text-slate-700">
            {t("summary")}
          </p>
          <div className="mt-4">
            <div className="text-xs font-medium text-slate-500">
              {t("stepsTitle")}
            </div>
            <ol className="mt-2 grid gap-2 text-sm text-slate-700">
              <li className="flex gap-2">
                <StepNumber value="1" />
                <span>
                  {selectedTenant
                    ? t("stepCreate", { name: selectedTenant.display_name })
                    : t("stepSelectTenant")}
                </span>
              </li>
              <li className="flex gap-2">
                <StepNumber value="2" />
                <span>{t("stepCopy")}</span>
              </li>
              <li className="flex gap-2">
                <StepNumber value="3" />
                <span>{t("stepStart")}</span>
              </li>
            </ol>
          </div>
        </div>
        <div className="bg-slate-50 px-4 py-4">
          {selectedTenant && !restricted ? (
            <CreateAgentPairingForm tenantId={selectedTenant.id} />
          ) : (
            <div className="text-sm">
              <div className="font-medium text-slate-950">
                {selectedTenant ? t("restrictedTitle") : t("noTenantTitle")}
              </div>
              <p className="mt-1 text-slate-600">
                {selectedTenant ? t("restrictedDetail") : t("noTenantDetail")}
              </p>
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

function StepNumber({ value }: { value: string }) {
  return (
    <span className="mt-0.5 inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md border border-slate-300 bg-slate-50 text-[11px] font-medium tabular-nums text-slate-700">
      {value}
    </span>
  );
}
```

- [ ] **Step 5: Wire the component into the Agents view**

Modify `frontend/app/dashboard-view-content.tsx`:

Add the import near the existing local component imports:

```tsx
import { AgentPairingGuidance } from "./agent-pairing-guidance";
```

Update `AgentsView` destructuring from:

```tsx
function AgentsView({
  selectedTenant,
  agents,
  printers,
  selectedCommand,
  commandData,
}: DashboardViewContentProps) {
```

to:

```tsx
function AgentsView({
  selectedTenant,
  agents,
  printers,
  selectedCommand,
  commandData,
  adminUnavailable,
}: DashboardViewContentProps) {
```

Render the guidance before linked agents:

```tsx
  return (
    <>
      <AgentPairingGuidance selectedTenant={selectedTenant} restricted={adminUnavailable} />
      <LinkedAgentsSection selectedTenant={selectedTenant} agents={agents} />
      <DiagnosticsSection
```

- [ ] **Step 6: Run the focused test to verify it passes**

Run:

```bash
npm --prefix frontend test -- agent-pairing-guidance.test.tsx
```

Expected: PASS for all three Agents view pairing guidance tests.

- [ ] **Step 7: Update the roadmap**

Modify `docs/roadmap.md` by adding a concise completed item near the top of the current completed/recent work section:

```markdown
- Completed: Agents page now includes tenant-aware pairing guidance, restricted/no-tenant states, and in-context pairing creation for tenant admins.
```

If the roadmap has a specific “Next” or “Upcoming” section, add this concise next-step note there only if it fits the existing structure:

```markdown
- Next: Validate the pairing guidance copy against real install/runbook instructions when packaging docs are finalized.
```

- [ ] **Step 8: Run final frontend verification**

Run:

```bash
npm --prefix frontend test -- agent-pairing-guidance.test.tsx dashboard-shell.test.tsx
```

Expected: PASS for the focused guidance tests and the existing dashboard shell tests.

Run:

```bash
npm --prefix frontend run lint
```

Expected: PASS with no lint errors.

Run:

```bash
npm --prefix frontend run build
```

Expected: Next.js production build completes successfully, including the plugin-local build.

- [ ] **Step 9: Review the diff**

Run:

```bash
git status --short
git diff -- frontend/app/agent-pairing-guidance.tsx frontend/app/agent-pairing-guidance.test.tsx frontend/app/dashboard-view-content.tsx frontend/messages/en.json frontend/messages/zh.json docs/roadmap.md docs/superpowers/specs/2026-06-30-frontend-agent-pairing-guidance-design.md docs/superpowers/plans/2026-06-30-frontend-agent-pairing-guidance.md
sed -n '1,240p' frontend/app/agent-pairing-guidance.tsx
sed -n '1,260p' frontend/app/agent-pairing-guidance.test.tsx
```

Expected: `git status --short` shows only intended files changed, no staged files, and no unrelated untracked files. The diff plus explicit new-file reads are limited to the spec, plan, guidance component/test, Agents view wiring, localized copy, and roadmap update.
