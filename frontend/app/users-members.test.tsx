import type { ReactNode } from "react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import zh from "../messages/zh.json";
import type { Tenant, User, UserIdentity } from "./dashboard-types";
import { MembersSection } from "./users-members";

vi.mock("./admin-actions", () => ({
  createJoinLink: vi.fn(),
  removeTenantUser: vi.fn(),
  revokeJoinLink: vi.fn(),
  updateTenantUserRole: vi.fn(),
}));

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
    warning: vi.fn(),
  },
}));

const tenant: Tenant = {
  id: "tenant-1",
  slug: "factory",
  display_name: "Factory Floor",
  created_at: "2026-06-01T00:00:00Z",
};

const users: User[] = [
  {
    id: "user-v",
    tenant_id: tenant.id,
    email: "zoe@example.test",
    display_name: "Zoe Viewer",
    role: "viewer",
    created_at: "2026-06-02T00:00:00Z",
  },
  {
    id: "user-a",
    tenant_id: tenant.id,
    email: "ada@example.test",
    display_name: "Ada Admin",
    role: "tenant_admin",
    created_at: "2026-06-01T00:00:00Z",
  },
  {
    id: "user-o",
    tenant_id: tenant.id,
    email: "oliver@example.test",
    display_name: "Oliver Operator",
    role: "operator",
    created_at: "2026-06-03T00:00:00Z",
  },
];

const identities: UserIdentity[] = [
  {
    id: "identity-1",
    tenant_id: tenant.id,
    user_id: "user-a",
    provider: "clerk",
    subject: "clerk-subject-1",
    created_at: "2026-06-01T00:00:00Z",
  },
];

function renderSection({
  locale = "en",
  meEmail = null,
}: {
  locale?: string;
  meEmail?: string | null;
} = {}) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const content: ReactNode = (
    <MembersSection
      identities={identities}
      meEmail={meEmail}
      tenant={tenant}
      users={users}
    />
  );
  return render(
    <NextIntlClientProvider locale={locale} messages={locale === "zh" ? zh : en}>
      <QueryClientProvider client={queryClient}>{content}</QueryClientProvider>
    </NextIntlClientProvider>,
  );
}

describe("MembersSection", () => {
  it("sorts members by role and shows per-role filter counts", () => {
    renderSection();

    const rows = screen.getAllByRole("row");
    expect(rows[1]).toHaveTextContent("Ada Admin");
    expect(rows[2]).toHaveTextContent("Oliver Operator");
    expect(rows[3]).toHaveTextContent("Zoe Viewer");

    expect(screen.getByRole("button", { name: /All · 3/ })).toBeVisible();
    expect(
      screen.getByRole("button", { name: /Tenant admin · 1/ }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: /Operator · 1/ })).toBeVisible();
    expect(screen.getByRole("button", { name: /Viewer · 1/ })).toBeVisible();
  });

  it("filters members by the search query", async () => {
    const event = userEvent.setup();
    renderSection();

    await event.type(screen.getByLabelText("Search members"), "zoe");

    expect(screen.getByText("Zoe Viewer")).toBeVisible();
    expect(screen.queryByText("Ada Admin")).not.toBeInTheDocument();
    expect(screen.queryByText("Oliver Operator")).not.toBeInTheDocument();
  });

  it("shows an empty result state when nothing matches", async () => {
    const event = userEvent.setup();
    renderSection();

    await event.type(screen.getByLabelText("Search members"), "nobody");

    expect(screen.getByText("No matching members")).toBeVisible();
  });

  it("filters members by role chip", async () => {
    const event = userEvent.setup();
    renderSection();

    await event.click(screen.getByRole("button", { name: /Viewer · 1/ }));

    expect(screen.getByText("Zoe Viewer")).toBeVisible();
    expect(screen.queryByText("Ada Admin")).not.toBeInTheDocument();
  });

  it("marks the current user with a You badge", () => {
    renderSection({ meEmail: "ada@example.test" });

    expect(screen.getByText("You")).toBeVisible();
  });

  it("opens the member detail dialog with identities", async () => {
    const event = userEvent.setup();
    renderSection();

    await event.click(
      screen.getByRole("button", { name: "Manage Ada Admin" }),
    );

    expect(await screen.findByText("Linked identities")).toBeVisible();
    expect(screen.getByText("clerk-subject-1")).toBeVisible();
    expect(screen.getByText("User ID")).toBeVisible();
  });

  it("offers a confirmed remove action for ordinary members", async () => {
    const event = userEvent.setup();
    renderSection();

    await event.click(
      screen.getByRole("button", { name: "Manage Oliver Operator" }),
    );

    const removeButton = await screen.findByRole("button", {
      name: "Remove Oliver Operator",
    });
    expect(removeButton).toBeVisible();

    await event.click(removeButton);
    expect(
      await screen.findByText(/Remove Oliver Operator from this tenant/),
    ).toBeVisible();
  });

  it("blocks removing the last tenant admin", async () => {
    const event = userEvent.setup();
    renderSection();

    await event.click(
      screen.getByRole("button", { name: "Manage Ada Admin" }),
    );

    expect(
      await screen.findByText("The last tenant admin can't be removed."),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "Remove Ada Admin" }),
    ).not.toBeInTheDocument();
  });

  it("blocks removing yourself", async () => {
    const event = userEvent.setup();
    renderSection({ meEmail: "oliver@example.test" });

    await event.click(
      screen.getByRole("button", { name: "Manage Oliver Operator" }),
    );

    expect(
      await screen.findByText("You can't remove your own membership."),
    ).toBeVisible();
  });

  it("localizes the filter in zh", () => {
    renderSection({ locale: "zh" });

    expect(screen.getByRole("button", { name: /全部 · 3/ })).toBeVisible();
  });
});
