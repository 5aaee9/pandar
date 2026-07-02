import { StrictMode } from "react";
import { NextIntlClientProvider } from "next-intl";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { actionStatusTone, formatActionStatus } from "./action-status";
import { ActionStatusToast } from "./action-status-toast";
import { DashboardShellHeader } from "./dashboard-shell-header";
import type { DashboardQuery, DashboardView } from "./dashboard-shell";
import type { Tenant } from "./dashboard-types";
import { toast } from "sonner";

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: vi.fn() }),
}));

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    warning: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock("../components/ui/sidebar", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../components/ui/sidebar")>();
  return {
    ...actual,
    SidebarTrigger: ({ className }: { className?: string }) => (
      <button aria-label="Toggle sidebar" className={className} type="button" />
    ),
  };
});

function renderWithMessages(children: React.ReactNode) {
  return render(
    <NextIntlClientProvider locale="en" messages={en}>
      {children}
    </NextIntlClientProvider>,
  );
}

function setUrl(path: string) {
  window.history.pushState({}, "", path);
}

function stubLocationAssign(assign: (url: string) => void) {
  const originalWindow = window;
  const location = {
    ...originalWindow.location,
    assign,
  };

  vi.stubGlobal(
    "window",
    new Proxy(originalWindow, {
      get(target, prop, receiver) {
        if (prop === "location") {
          return location;
        }
        return Reflect.get(target, prop, receiver);
      },
    }),
  );
}

afterEach(() => {
  vi.clearAllMocks();
  vi.unstubAllGlobals();
  window.history.replaceState({}, "", "/");
});

describe("action status toast helpers", () => {
  it("formats translated and fallback status messages", () => {
    const tStatus = Object.assign(
      (key: string) => en.runtime.actionStatus[key as keyof typeof en.runtime.actionStatus],
      { has: (key: string) => key in en.runtime.actionStatus },
    );

    expect(formatActionStatus("refresh_queued", tStatus)).toBe("Refresh queued");
    expect(formatActionStatus("agent_deleted", tStatus)).toBe("Agent deleted");
    expect(formatActionStatus("agent_not_connected", tStatus)).toBe("Agent is not connected to this Hub process");
    expect(formatActionStatus("artifact_too_large", tStatus)).toBe("Artifact Too Large");
  });

  it("classifies status tone deterministically", () => {
    expect(actionStatusTone("refresh_queued")).toBe("success");
    expect(actionStatusTone("materials_refresh_queued")).toBe("success");
    expect(actionStatusTone("agent_deleted")).toBe("success");
    expect(actionStatusTone("refresh_partial")).toBe("warning");
    expect(actionStatusTone("http_500")).toBe("error");
    expect(actionStatusTone("agent_not_connected")).toBe("error");
    expect(actionStatusTone("artifact_too_large")).toBe("error");
  });
});

describe("ActionStatusToast", () => {
  it("shows a success toast and clears only the status query parameter", async () => {
    setUrl("/devices?tenant=t1&status=refresh_queued");

    renderWithMessages(<ActionStatusToast status="refresh_queued" />);

    await waitFor(() => expect(toast.success).toHaveBeenCalledWith("Refresh queued"));
    expect(window.location.pathname + window.location.search).toBe("/devices?tenant=t1");
  });

  it("shows a warning toast and preserves tenant plus command query parameters", async () => {
    setUrl("/devices?tenant=t1&command=c1&status=refresh_partial");

    renderWithMessages(<ActionStatusToast status="refresh_partial" />);

    await waitFor(() =>
      expect(toast.warning).toHaveBeenCalledWith(
        "Some refreshes could not be queued — review the list",
      ),
    );
    expect(window.location.pathname + window.location.search).toBe("/devices?tenant=t1&command=c1");
  });

  it("shows an error toast for unexpected backend error codes", async () => {
    setUrl("/devices?tenant=t1&status=artifact_too_large");

    renderWithMessages(<ActionStatusToast status="artifact_too_large" />);

    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Artifact Too Large"));
  });

  it("does not duplicate toasts under Strict Mode effect replay", async () => {
    setUrl("/devices?tenant=t1&status=refresh_queued");

    renderWithMessages(
      <StrictMode>
        <ActionStatusToast status="refresh_queued" />
      </StrictMode>,
    );

    await waitFor(() => expect(toast.success).toHaveBeenCalledTimes(1));
  });
});

const tenants: Tenant[] = [
  { id: "t1", slug: "tenant-one", display_name: "Tenant One", created_at: "2026-06-30T00:00:00Z" },
  { id: "t2", slug: "tenant-two", display_name: "Tenant Two", created_at: "2026-06-30T00:00:00Z" },
];

function DashboardHeaderWithActionStatus({ actionStatus }: { actionStatus: string }) {
  const query: DashboardQuery = {
    tenant: "t1",
    command: "cmd1",
  };
  const view: DashboardView = "agents";
  return (
    <>
      <ActionStatusToast status={actionStatus} />
      <DashboardShellHeader query={query} selectedTenant={tenants[0]} tenants={tenants} view={view} />
    </>
  );
}

describe("action status navigation", () => {
  it("does not preserve action status when switching tenants", async () => {
    const user = userEvent.setup();
    const assign = vi.fn<(url: string) => void>();
    stubLocationAssign(assign);
    setUrl("/agents?tenant=t1&command=cmd1&status=refresh_queued");

    renderWithMessages(<DashboardHeaderWithActionStatus actionStatus="refresh_queued" />);
    await waitFor(() => expect(toast.success).toHaveBeenCalledTimes(1));

    await user.selectOptions(screen.getByRole("combobox"), "t2");

    expect(assign).toHaveBeenCalledWith("/agents?tenant=t2&command=cmd1");
  });
});
