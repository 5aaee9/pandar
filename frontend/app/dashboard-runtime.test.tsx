import { NextIntlClientProvider } from "next-intl";
import { render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { DashboardRuntime } from "./dashboard-runtime";
import type { AuthMetadata, Tenant } from "./dashboard-types";

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: vi.fn() }),
}));

const tenant: Tenant = {
  id: "t1",
  slug: "tenant-one",
  display_name: "Tenant One",
  created_at: "2026-06-30T00:00:00Z",
};

const noAuth: AuthMetadata = {
  source: "none",
  cookieName: "pandar_auth",
  provider: "none",
  signInUrl: null,
  signOutUrl: null,
};

function renderRuntime(auth: AuthMetadata = noAuth) {
  return render(
    <NextIntlClientProvider locale="en" messages={en}>
      <DashboardRuntime
        apiUrl="http://localhost:8080"
        view="devices"
        summary={null}
        tenants={[tenant]}
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
        selectedCommand={null}
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
});
