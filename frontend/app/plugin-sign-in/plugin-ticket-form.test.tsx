import { render } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { expect, it, vi } from "vitest";

import en from "../../messages/en.json";
import { PluginTicketForm } from "./plugin-ticket-form";

it("keeps the supplied exact callback and never re-probes Studio", () => {
  const postMessage = vi.fn();
  Object.defineProperty(window, "wx", {
    configurable: true,
    value: { postMessage },
  });
  const redirectUrl = "http://127.0.0.1:13618/callback";

  const { container, unmount } = render(
    <NextIntlClientProvider locale="en" messages={en}>
      <PluginTicketForm
        action={vi.fn(async () => undefined)}
        autoSelectedTenant
        redirectUrl={redirectUrl}
        selectedTenant={{
          id: "tenant-1",
          slug: "factory",
          display_name: "Factory",
          created_at: "2026-08-30T00:00:00Z",
        }}
      />
    </NextIntlClientProvider>,
  );

  expect(
    container.querySelector<HTMLInputElement>('input[name="redirect_url"]')
      ?.value,
  ).toBe(redirectUrl);
  expect(postMessage).not.toHaveBeenCalled();
  unmount();
  Reflect.deleteProperty(window, "wx");
});
