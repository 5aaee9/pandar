import { NextIntlClientProvider } from "next-intl";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { updateTenantDisplayName } from "./admin-actions";
import type { Tenant } from "./dashboard-types";
import { WorkspaceRenameForm } from "./settings-rename-form";

vi.mock("./admin-actions", () => ({
  updateTenantDisplayName: vi.fn(),
}));

const toastMock = vi.hoisted(() => ({ success: vi.fn() }));
vi.mock("sonner", () => ({
  toast: toastMock,
}));

const updateMock = vi.mocked(updateTenantDisplayName);

const tenant: Tenant = {
  id: "tenant-1",
  slug: "maker-lab",
  display_name: "Maker Lab",
  created_at: "2026-06-30T00:00:00Z",
};

function renderForm() {
  return render(
    <NextIntlClientProvider locale="en" messages={en}>
      <WorkspaceRenameForm tenant={tenant} />
    </NextIntlClientProvider>,
  );
}

describe("WorkspaceRenameForm", () => {
  it("keeps save disabled until the name changes", async () => {
    renderForm();
    const saveButton = screen.getByRole("button", { name: "Save" });
    expect(saveButton).toBeDisabled();

    await userEvent.type(
      screen.getByRole("textbox", { name: "Workspace name" }),
      " 2",
    );
    expect(saveButton).toBeEnabled();
  });

  it("shows a success toast after renaming", async () => {
    updateMock.mockResolvedValue({ ok: true });
    renderForm();

    const input = screen.getByRole("textbox", { name: "Workspace name" });
    await userEvent.clear(input);
    await userEvent.type(input, "Studio");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(toastMock.success).toHaveBeenCalled());
  });

  it("shows an inline error when renaming fails", async () => {
    updateMock.mockResolvedValue({ ok: false, error: "role_forbidden" });
    renderForm();

    const input = screen.getByRole("textbox", { name: "Workspace name" });
    await userEvent.clear(input);
    await userEvent.type(input, "Studio");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(
      await screen.findByRole("alert"),
    ).toHaveTextContent("Only workspace administrators can rename the workspace.");
  });
});
