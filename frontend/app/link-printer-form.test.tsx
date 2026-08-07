import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { LinkPrinterMachineForm } from "./link-printer-form";
import { linkPrinter } from "./actions";
import type { Agent, Command, Tenant } from "./dashboard-types";

vi.mock("./actions", () => ({
  linkPrinter: vi.fn(),
}));

const toastMock = vi.hoisted(() => ({ success: vi.fn(), error: vi.fn() }));
vi.mock("sonner", () => ({
  toast: toastMock,
}));

const linkPrinterMock = vi.mocked(linkPrinter);

const tenant: Tenant = {
  id: "tenant-1",
  slug: "factory",
  display_name: "Factory Floor",
  created_at: "2026-06-30T00:00:00Z",
};

const agent: Agent = {
  id: "agent-1",
  tenant_id: tenant.id,
  name: "Lab agent",
  status: "online",
  created_at: "2026-06-30T00:00:00Z",
};

function linkCommand(overrides: Partial<Command>): Command {
  return {
    id: "cmd-link",
    tenant_id: tenant.id,
    agent_id: agent.id,
    printer_id: null,
    kind: "link_printer",
    status: "sent",
    payload_json: "{}",
    error: null,
    result_json: null,
    created_at: "2026-07-02T00:00:00Z",
    updated_at: "2026-07-02T00:00:05Z",
    ...overrides,
  };
}

function renderForm(onLinked?: () => void) {
  const queryClient = new QueryClient();
  const user = userEvent.setup();
  render(
    <NextIntlClientProvider locale="en" messages={en}>
      <QueryClientProvider client={queryClient}>
        <LinkPrinterMachineForm
          agents={[agent]}
          onLinked={onLinked}
          selectedTenant={tenant}
        />
      </QueryClientProvider>
    </NextIntlClientProvider>,
  );
  return { user, queryClient };
}

async function submitForm(user: ReturnType<typeof userEvent.setup>) {
  await user.type(screen.getByLabelText("Printer IPv4 address"), "192.0.2.10");
  await user.type(screen.getByLabelText("Access code"), "SECRET-LINK-CODE");
  await user.click(screen.getByRole("button", { name: "Link printer" }));
}

describe("LinkPrinterMachineForm", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    linkPrinterMock.mockResolvedValue({ ok: true, commandId: "cmd-link" });
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        Response.json(linkCommand({ status: "succeeded" })),
      ),
    );
  });

  it("keeps the dialog open with a loading button until the agent confirms the link", async () => {
    let resolveAction: (value: { ok: true; commandId: string }) => void = () => {};
    linkPrinterMock.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveAction = resolve;
        }),
    );
    const onLinked = vi.fn();
    const { user } = renderForm(onLinked);

    await submitForm(user);

    const button = screen.getByRole("button", { name: /Linking/ });
    expect(button).toBeDisabled();
    expect(onLinked).not.toHaveBeenCalled();

    resolveAction({ ok: true, commandId: "cmd-link" });

    await waitFor(() => expect(onLinked).toHaveBeenCalledTimes(1));
    expect(toastMock.success).toHaveBeenCalledWith(
      "Printer linked to the agent.",
    );
  });

  it("toggles access-code visibility without changing its value", async () => {
    const { user } = renderForm();
    const input = screen.getByLabelText("Access code");

    await user.type(input, "SECRET-LINK-CODE");
    expect(input).toHaveAttribute("type", "password");

    await user.click(screen.getByRole("button", { name: "Show access code" }));
    expect(input).toHaveAttribute("type", "text");
    expect(input).toHaveValue("SECRET-LINK-CODE");

    await user.click(screen.getByRole("button", { name: "Hide access code" }));
    expect(input).toHaveAttribute("type", "password");
    expect(input).toHaveValue("SECRET-LINK-CODE");
    expect(linkPrinterMock).not.toHaveBeenCalled();
  });

  it("shows the error code when the printer rejects the access code", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        Response.json(
          linkCommand({
            status: "failed",
            error: "validate runtime printer SERIAL123: connection refused",
            result_json: JSON.stringify({
              type: "printer_link_error",
              error_code: "invalid_access_code",
            }),
          }),
        ),
      ),
    );
    const onLinked = vi.fn();
    const { user } = renderForm(onLinked);

    await submitForm(user);

    await screen.findByText(
      "Error: The printer rejected the access code. Check the 8-digit access code on the printer screen or in Bambu Handy. (invalid_access_code)",
    );
    expect(onLinked).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Access code")).toHaveValue(
      "SECRET-LINK-CODE",
    );
    expect(
      screen.getByRole("button", { name: "Link printer" }),
    ).toBeEnabled();
  });

  it("shows the hub error code when the dispatch is rejected", async () => {
    linkPrinterMock.mockResolvedValue({ ok: false, error: "agent_not_connected" });
    const onLinked = vi.fn();
    const { user } = renderForm(onLinked);

    await submitForm(user);

    await screen.findByText(
      "Error: The agent is not connected. Wait for it to come online, then try again. (agent_not_connected)",
    );
    expect(onLinked).not.toHaveBeenCalled();
  });

  it("shows the detailed cause when an unclassified link failure occurs", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        Response.json(
          linkCommand({
            status: "failed",
            error: "complete Bambu MQTT TLS handshake: Certificate is not valid",
            result_json: JSON.stringify({
              type: "printer_link_error",
              error_code: "link_failed",
            }),
          }),
        ),
      ),
    );
    const { user } = renderForm();

    await submitForm(user);

    await screen.findByText(
      "Error: The printer could not be linked: complete Bambu MQTT TLS handshake: Certificate is not valid (link_failed)",
    );
  });

  it("falls back to the raw error when the failure has no error code", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        Response.json(
          linkCommand({
            status: "failed",
            error: "agent connection closed before printer link completed",
          }),
        ),
      ),
    );
    const { user } = renderForm();

    await submitForm(user);

    await screen.findByText(
      "Error: agent connection closed before printer link completed",
    );
  });
});
