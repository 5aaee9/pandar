import { NextIntlClientProvider } from "next-intl";
import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { DashboardRuntime } from "./dashboard-runtime";
import type { AuthMetadata, Tenant } from "./dashboard-types";
import type { DashboardView } from "./dashboard-shell";

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: vi.fn() }),
}));

const tenant: Tenant = {
  id: "t1",
  slug: "tenant-one",
  display_name: "Tenant One",
  created_at: "2026-06-30T00:00:00Z",
};

const otherTenant: Tenant = {
  id: "t2",
  slug: "tenant-two",
  display_name: "Tenant Two",
  created_at: "2026-06-30T00:00:00Z",
};

const noAuth: AuthMetadata = {
  source: "none",
  cookieName: "pandar_auth",
  provider: "none",
  signInUrl: null,
  signOutUrl: null,
};

function renderRuntime(
  auth: AuthMetadata = noAuth,
  options: {
    view?: DashboardView;
    actionStatus?: string;
    selectedCommandId?: string;
    tenants?: Tenant[];
  } = {},
) {
  return render(
    <NextIntlClientProvider locale="en" messages={en}>
      <DashboardRuntime
        apiUrl="http://localhost:8080"
        view={options.view ?? "devices"}
        summary={null}
        tenants={options.tenants ?? [tenant]}
        selectedTenant={tenant}
        initialPrinters={[]}
        agents={[]}
        initialJobs={[]}
        users={[]}
        userIdentities={[]}
        tenantTokens={[]}
        joinLinks={[]}
        auditEvents={[]}
        adminUnavailable={false}
        actionStatus={options.actionStatus}
        selectedCommand={null}
        selectedCommandId={options.selectedCommandId}
        commandData={null}
        errors={[]}
        auth={auth}
      />
    </NextIntlClientProvider>,
  );
}

describe("DashboardRuntime live connection", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("connects directly to printer events when hub auth is disabled", async () => {
    const urls: string[] = [];
    vi.stubGlobal("fetch", vi.fn());
    vi.stubGlobal(
      "WebSocket",
      class {
        onopen: (() => void) | null = null;

        constructor(url: string) {
          urls.push(url);
        }

        close() {}
      },
    );

    renderRuntime();

    await waitFor(() => {
      expect(urls).toEqual([
        "ws://localhost:8080/api/v1/tenants/t1/printer-events",
      ]);
    });
    expect(fetch).not.toHaveBeenCalled();
  });

  it("preserves action status when switching tenants from jobs", () => {
    vi.stubGlobal("fetch", vi.fn());
    vi.stubGlobal(
      "WebSocket",
      class {
        close() {}
      },
    );

    renderRuntime(noAuth, {
      view: "jobs",
      actionStatus: "refresh_queued",
      selectedCommandId: "cmd1",
      tenants: [tenant, otherTenant],
    });

    expect(screen.getByRole("link", { name: "Tenant Two" })).toHaveAttribute(
      "href",
      "/jobs?tenant=t2&status=refresh_queued",
    );
  });
});
