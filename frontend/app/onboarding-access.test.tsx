import { NextIntlClientProvider } from "next-intl";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import en from "../messages/en.json";
import { TenantAccessSwitcher } from "./tenant-access-switcher";
import { JoinTokenForm } from "./join/token-form";

function renderWithMessages(children: React.ReactNode) {
  return render(
    <NextIntlClientProvider locale="en" messages={en}>
      {children}
    </NextIntlClientProvider>,
  );
}

describe("tenant onboarding access", () => {
  it("opens a switcher menu with create and join tenant actions", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <TenantAccessSwitcher
        createAction={vi.fn()}
        identityEmail="operator@example.com"
      />,
    );

    const trigger = screen.getByRole("button", {
      name: "Select tenant access action",
    });
    expect(trigger).toHaveAttribute("aria-haspopup", "menu");
    await user.click(trigger);

    expect(screen.getByRole("menu")).toBeVisible();
    expect(
      screen.getByRole("menuitem", { name: /create tenant/i }),
    ).toBeVisible();
    expect(
      screen.getByRole("menuitem", { name: /join tenant/i }),
    ).toHaveAttribute("href", "/join");
  });

  it("opens the create tenant dialog with real onboarding fields", async () => {
    const user = userEvent.setup();
    renderWithMessages(
      <TenantAccessSwitcher
        createAction={vi.fn()}
        identityEmail="operator@example.com"
      />,
    );

    await user.click(
      screen.getByRole("button", { name: "Select tenant access action" }),
    );
    await user.click(screen.getByRole("menuitem", { name: /create tenant/i }));

    expect(
      screen.getByRole("dialog", { name: /create tenant/i }),
    ).toBeVisible();
    expect(screen.getByLabelText("Tenant name")).toHaveAttribute(
      "name",
      "display_name",
    );
    expect(screen.getByLabelText("Tenant slug")).toHaveAttribute(
      "name",
      "slug",
    );
    expect(
      screen.getByRole("button", { name: /^create tenant$/i }),
    ).toHaveAttribute("type", "submit");
  });

  it("pre-fills join token input from the URL hash", () => {
    window.location.hash = "invite-token-123";

    renderWithMessages(<JoinTokenForm action={vi.fn()} />);

    expect(screen.getByLabelText("Join token")).toHaveValue(
      "invite-token-123",
    );
    expect(screen.getByRole("button", { name: /join tenant/i })).toHaveAttribute(
      "type",
      "submit",
    );
  });
});
