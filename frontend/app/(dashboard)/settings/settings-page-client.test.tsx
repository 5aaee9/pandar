import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { afterEach, describe, expect, it, vi } from "vitest";

import en from "../../../messages/en.json";
import type { AuthMetadata, Tenant } from "../../dashboard-types";
import { SettingsPageClient } from "./settings-page-client";

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: vi.fn() }),
}));

const auth: AuthMetadata = {
  source: "request_cookie",
  cookieName: "pandar_auth",
  provider: "clerk",
  signInUrl: "/sign-in",
  signOutUrl: "/sign-out",
};

const tenant: Tenant = {
  id: "t1",
  slug: "maker-lab",
  display_name: "Maker Lab",
  created_at: "2026-06-30T00:00:00Z",
};

function renderSettings(
  membership: { role: string | null; error: string | null },
  authMetadata: AuthMetadata = auth,
) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <NextIntlClientProvider locale="en" messages={en}>
        <SettingsPageClient
          auth={authMetadata}
          membership={membership}
          selectedTenant={tenant}
        />
      </NextIntlClientProvider>
    </QueryClientProvider>,
  );
}

function stubWorkspaceFetch() {
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    const path = String(input);
    if (path.endsWith("/agents")) {
      return Response.json({ agents: [] });
    }
    if (path.endsWith("/printers")) {
      return Response.json({ printers: [] });
    }
    throw new Error(`unexpected admin request: ${path}`);
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

function expectNoAdminRequests(fetchMock: ReturnType<typeof stubWorkspaceFetch>) {
  expect(fetchMock.mock.calls.map(([input]) => String(input))).not.toEqual(
    expect.arrayContaining([
      expect.stringContaining("tenant-tokens"),
      expect.stringContaining("audit-events"),
    ]),
  );
}

describe("SettingsPageClient authorization states", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("keeps workspace settings available when membership lookup fails", async () => {
    const fetchMock = stubWorkspaceFetch();
    renderSettings({ role: null, error: "membership request failed" });

    expect(
      await screen.findByRole("heading", { name: "Settings" }),
    ).toBeVisible();
    expect(screen.getByText("Role unavailable")).toBeVisible();
    expect(
      screen.getByText("Security settings could not be loaded"),
    ).toBeVisible();
    expect(
      screen.queryByRole("textbox", { name: "Workspace name" }),
    ).not.toBeInTheDocument();
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    expectNoAdminRequests(fetchMock);
  });

  it("shows restricted administration without requesting protected data", async () => {
    const fetchMock = stubWorkspaceFetch();
    renderSettings({ role: "viewer", error: null });

    expect(await screen.findByText("Viewer")).toBeVisible();
    expect(screen.getByText("Administrator access required")).toBeVisible();
    expect(
      screen.queryByRole("textbox", { name: "Workspace name" }),
    ).not.toBeInTheDocument();
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    expectNoAdminRequests(fetchMock);
  });

  it("offers the rename form to workspace administrators", async () => {
    stubWorkspaceFetch();
    renderSettings({ role: "tenant_admin", error: null });

    expect(
      await screen.findByRole("textbox", { name: "Workspace name" }),
    ).toHaveValue("Maker Lab");
  });
});
