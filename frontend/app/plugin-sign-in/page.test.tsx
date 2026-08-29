import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const fetchMock = vi.hoisted(() => vi.fn());

vi.mock("next-intl/server", () => ({
  getTranslations: vi.fn(async () => (key: string) => key),
}));

vi.mock("../actions", () => ({
  createMobileTicket: vi.fn(),
  createPluginTicket: vi.fn(),
}));

vi.mock("../api-auth", () => ({
  apiHeaders: vi.fn(async () => ({ authorization: "Bearer external-jwt" })),
  authSource: vi.fn(async () => ({
    source: "request_cookie",
    cookieName: "pandar_auth_token",
    provider: "betterauth",
  })),
}));

vi.mock("../auth-provider", () => ({
  authProviderConfig: vi.fn(() => ({
    provider: "betterauth",
    signInUrl: "https://auth.example/sign-in",
  })),
}));

vi.mock("../../components/language-switcher", () => ({
  LanguageSwitcher: () => <div />,
}));

vi.mock("./external-auth-status", () => ({
  fetchExternalAuthStatus: vi.fn(async () => ({
    externalAuthEnabled: true,
    error: null,
  })),
}));

vi.mock("./plugin-ticket-form", () => ({
  PluginTicketForm: ({
    selectedTenant,
  }: {
    selectedTenant: { display_name: string };
  }) => <div>plugin ticket tenant: {selectedTenant.display_name}</div>,
}));

vi.mock("../mobile-sign-in/mobile-ticket-form", () => ({
  MobileTicketForm: ({
    selectedTenant,
  }: {
    selectedTenant: { display_name: string };
  }) => <div>mobile ticket tenant: {selectedTenant.display_name}</div>,
}));

describe("PluginSignInPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("fetch", fetchMock);
    fetchMock.mockImplementation(async (input: string | URL | Request) => {
      const url = input.toString();
      if (url.endsWith("/api/v1/me")) {
        return new Response(
          JSON.stringify({
            identity: {
              provider: "betterauth",
              subject: "user-1",
              email: "user@example.com",
              email_verified: true,
              display_name: "User",
            },
            tenants: [
              {
                tenant_id: "tenant-1",
                tenant_slug: "acme",
                display_name: "Acme Labs",
                role: "tenant_admin",
              },
            ],
            can_self_create_tenant: true,
          }),
          { status: 200 },
        );
      }
      if (url.endsWith("/api/v1/tenants")) {
        return new Response(JSON.stringify({ error: "forbidden" }), {
          status: 403,
        });
      }
      throw new Error(`Unexpected fetch: ${url}`);
    });
  });

  it("uses the signed-in identity memberships instead of the bootstrap tenant list", async () => {
    const { default: PluginSignInPage } = await import("./page");

    render(await PluginSignInPage({ searchParams: Promise.resolve({}) }));

    expect(
      screen.queryByText("Tenant lookup returned 403"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText("plugin ticket tenant: Acme Labs"),
    ).toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/me",
      expect.objectContaining({ cache: "no-store" }),
    );
    expect(fetchMock).not.toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/tenants",
      expect.anything(),
    );
  });

  it("preserves the callback while selecting among identity tenants", async () => {
    fetchMock.mockImplementation(async (input: string | URL | Request) => {
      const url = input.toString();
      if (url.endsWith("/api/v1/me")) {
        return new Response(
          JSON.stringify({
            identity: {
              provider: "betterauth",
              subject: "user-1",
              email: "user@example.com",
              email_verified: true,
              display_name: "User",
            },
            tenants: [
              {
                tenant_id: "tenant-1",
                tenant_slug: "acme",
                display_name: "Acme Labs",
                role: "tenant_admin",
              },
              {
                tenant_id: "tenant-2",
                tenant_slug: "workshop",
                display_name: "Workshop",
                role: "operator",
              },
            ],
            can_self_create_tenant: true,
          }),
          { status: 200 },
        );
      }
      throw new Error(`Unexpected fetch: ${url}`);
    });
    const { default: PluginSignInPage } = await import("./page");

    const view = render(
      await PluginSignInPage({
        searchParams: Promise.resolve({
          redirect_url: "http://127.0.0.1:13618/callback",
        }),
      }),
    );

    expect(screen.getByRole("combobox", { name: "tenant" })).toHaveValue(
      "tenant-1",
    );
    expect(
      screen.getByRole("option", { name: "Workshop" }),
    ).toBeInTheDocument();
    expect(
      view.container.querySelector<HTMLInputElement>(
        'input[name="redirect_url"]',
      ),
    ).toHaveValue("http://127.0.0.1:13618/callback");
  });

  it("uses identity memberships for mobile sign-in too", async () => {
    const { default: MobileSignInPage } =
      await import("../mobile-sign-in/page");

    render(await MobileSignInPage({ searchParams: Promise.resolve({}) }));

    expect(
      screen.queryByText("Tenant lookup returned 403"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText("mobile ticket tenant: Acme Labs"),
    ).toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledWith(
      "http://localhost:8080/api/v1/me",
      expect.objectContaining({ cache: "no-store" }),
    );
  });
});
