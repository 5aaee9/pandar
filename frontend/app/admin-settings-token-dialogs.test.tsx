import { act } from "react";
import { NextIntlClientProvider } from "next-intl";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import type { SecretActionState } from "./actions";
import { SecretActionResult } from "./admin-panel-shared";
import { TenantSecretsPanel } from "./admin-settings-panel";
import type { Tenant, TenantToken } from "./dashboard-types";

const actionMocks = vi.hoisted(() => ({
  createAgentPairing: vi.fn(),
  createTenantToken: vi.fn(),
  revokeTenantToken: vi.fn(),
  rotateTenantToken: vi.fn(),
}));

vi.mock("./actions", () => actionMocks);

const tenant: Tenant = {
  id: "tenant-1",
  slug: "factory",
  display_name: "Factory",
  created_at: "2026-07-01T00:00:00Z",
};

const nowMs = Date.parse("2026-07-17T00:00:00Z");
const activeToken: TenantToken = {
  id: "token-old",
  tenant_id: tenant.id,
  name: "Studio token",
  scopes: ["plugin:studio"],
  created_by_user_id: null,
  created_at: "2026-07-16T00:00:00Z",
  last_used_at: null,
  expires_at: "2027-01-01T00:00:00Z",
  revoked_at: null,
};
const rotatedToken: TenantToken = {
  ...activeToken,
  id: "token-new",
  created_at: "2026-07-17T00:00:00Z",
};

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, reject, resolve };
}

function TokenPanel({ tokens }: { tokens: TenantToken[] }) {
  return (
    <NextIntlClientProvider locale="en" messages={en}>
      <TenantSecretsPanel
        selectedTenant={tenant}
        tenantTokens={tokens}
        nowMs={nowMs}
      />
    </NextIntlClientProvider>
  );
}

describe("tenant token dialogs", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("locks creation while pending and clears the one-time secret after closing", async () => {
    const user = userEvent.setup();
    const request = deferred<SecretActionState>();
    actionMocks.createTenantToken.mockImplementation(() => request.promise);
    render(<TokenPanel tokens={[activeToken]} />);

    await user.click(screen.getByRole("button", { name: "Create token" }));
    const dialog = screen.getByRole("dialog", {
      name: "Create tenant token",
    });
    await user.click(
      within(dialog).getByRole("button", { name: "Create token" }),
    );

    await waitFor(() => {
      expect(actionMocks.createTenantToken).toHaveBeenCalledTimes(1);
      expect(
        within(dialog).getByRole("button", { name: "Creating..." }),
      ).toBeDisabled();
    });
    expect(
      within(dialog).getByRole("button", { name: "Cancel" }),
    ).toBeDisabled();
    expect(
      within(dialog).queryByRole("button", { name: "Close" }),
    ).not.toBeInTheDocument();

    await user.keyboard("{Escape}");
    expect(dialog).toBeVisible();

    const createdToken = { ...rotatedToken, id: "token-created" };
    await act(async () => {
      request.resolve({
        ok: true,
        kind: "tenant_token",
        operation: "created",
        tenantToken: createdToken,
        token: "pandar_tenant_create-once",
      });
      await request.promise;
    });
    expect(screen.getByText("pandar_tenant_create-once")).toBeVisible();
    expect(
      screen.getByText("pandar_tenant_create-once").closest('[data-motion="secret-result"]'),
    ).toBeVisible();
    expect(within(dialog).getByLabelText("Name")).toBeDisabled();
    expect(within(dialog).getByLabelText("Scopes")).toBeDisabled();
    expect(within(dialog).getByLabelText("Expires at")).toBeDisabled();
    expect(
      within(dialog).queryByRole("button", { name: "Create token" }),
    ).not.toBeInTheDocument();
    expect(
      within(dialog).queryByRole("button", { name: "Cancel" }),
    ).not.toBeInTheDocument();
    expect(dialog.querySelector('button[type="submit"]')).not.toBeInTheDocument();

    await user.keyboard("{Enter}");
    fireEvent.submit(dialog.querySelector("form")!);
    expect(actionMocks.createTenantToken).toHaveBeenCalledTimes(1);

    const createFooter = dialog.querySelector(
      '[data-slot="dialog-footer"]',
    ) as HTMLElement;
    expect(
      within(createFooter).getByRole("button", { name: "Close" }),
    ).toBeVisible();
    await user.click(
      within(createFooter).getByRole("button", { name: "Close" }),
    );
    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "Create token" }));

    expect(
      screen.getByRole("dialog", { name: "Create tenant token" }),
    ).toBeVisible();
    expect(
      screen.queryByText("pandar_tenant_create-once"),
    ).not.toBeInTheDocument();
  });

  it("keeps the rotated secret visible when refreshed props revoke the old token", async () => {
    const user = userEvent.setup();
    const request = deferred<SecretActionState>();
    actionMocks.rotateTenantToken.mockImplementation(() => request.promise);
    const view = render(<TokenPanel tokens={[activeToken]} />);
    const row = screen.getByText("Studio token").closest("article")!;

    await user.click(within(row).getByRole("button", { name: "Rotate" }));
    const dialog = screen.getByRole("dialog", { name: "Rotate tenant token" });
    await user.click(
      within(dialog).getByRole("button", { name: "Rotate token" }),
    );

    await waitFor(() => {
      expect(actionMocks.rotateTenantToken).toHaveBeenCalledTimes(1);
      expect(
        within(dialog).getByRole("button", { name: "Rotating..." }),
      ).toBeDisabled();
    });
    expect(
      within(dialog).getByRole("button", { name: "Cancel" }),
    ).toBeDisabled();
    expect(
      within(dialog).queryByRole("button", { name: "Close" }),
    ).not.toBeInTheDocument();

    await user.keyboard("{Escape}");
    expect(dialog).toBeVisible();

    await act(async () => {
      request.resolve({
        ok: true,
        kind: "tenant_token",
        operation: "rotated",
        tenantToken: rotatedToken,
        token: "pandar_tenant_rotate-once",
      });
      await request.promise;
    });
    expect(screen.getByText("pandar_tenant_rotate-once")).toBeVisible();
    expect(
      screen.getByText("pandar_tenant_rotate-once").closest('[data-motion="secret-result"]'),
    ).toBeVisible();
    expect(within(dialog).getByLabelText("Expires at")).toBeDisabled();
    expect(
      within(dialog).queryByRole("button", { name: "Rotate token" }),
    ).not.toBeInTheDocument();
    expect(
      within(dialog).queryByRole("button", { name: "Cancel" }),
    ).not.toBeInTheDocument();
    expect(dialog.querySelector('button[type="submit"]')).not.toBeInTheDocument();
    const rotateFooter = dialog.querySelector(
      '[data-slot="dialog-footer"]',
    ) as HTMLElement;
    expect(
      within(rotateFooter).getByRole("button", { name: "Close" }),
    ).toBeVisible();

    await user.keyboard("{Enter}");
    fireEvent.submit(dialog.querySelector("form")!);
    expect(actionMocks.rotateTenantToken).toHaveBeenCalledTimes(1);

    view.rerender(
      <TokenPanel
        tokens={[
          rotatedToken,
          { ...activeToken, revoked_at: "2026-07-17T00:00:00Z" },
        ]}
      />,
    );

    expect(screen.getByText("pandar_tenant_rotate-once")).toBeVisible();
    expect(
      screen.getByRole("dialog", { name: "Rotate tenant token" }),
    ).toBeVisible();
    expect(screen.getByTitle("token-new").closest("article")).toHaveAttribute(
      "data-token-status",
      "active",
    );
    expect(screen.getByTitle("token-old").closest("article")).toHaveAttribute(
      "data-token-status",
      "revoked",
    );
  });

  it("keeps errors immediate without the successful-result motion marker", () => {
    render(
      <NextIntlClientProvider locale="en" messages={en}>
        <SecretActionResult state={{ ok: false, error: "Creation failed" }} />
      </NextIntlClientProvider>,
    );

    const error = screen.getByText("Creation failed");
    expect(error).toBeVisible();
    expect(error.closest("[data-motion]")).toBeNull();
  });

  it("locks revoke confirmation while its redirecting action is pending", async () => {
    const user = userEvent.setup();
    const request = deferred<void>();
    actionMocks.revokeTenantToken.mockImplementation(() => request.promise);
    render(<TokenPanel tokens={[activeToken]} />);
    const row = screen.getByText("Studio token").closest("article")!;

    await user.click(within(row).getByRole("button", { name: "Revoke" }));
    const dialog = screen.getByRole("dialog", { name: "Revoke tenant token" });
    await user.click(
      within(dialog).getByRole("button", { name: "Revoke token" }),
    );

    await waitFor(() => {
      expect(actionMocks.revokeTenantToken).toHaveBeenCalledTimes(1);
      expect(
        within(dialog).getByRole("button", { name: "Revoking..." }),
      ).toBeDisabled();
    });
    expect(
      within(dialog).getByRole("button", { name: "Cancel" }),
    ).toBeDisabled();
    expect(
      within(dialog).queryByRole("button", { name: "Close" }),
    ).not.toBeInTheDocument();

    await user.keyboard("{Escape}");
    expect(dialog).toBeVisible();

    await act(async () => {
      request.resolve();
      await request.promise;
    });
  });
});
