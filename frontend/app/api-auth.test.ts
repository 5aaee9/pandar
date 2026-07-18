import { beforeEach, describe, expect, it, vi } from "vitest";

const cookiesMock = vi.hoisted(() => vi.fn());
const redirectMock = vi.hoisted(() =>
  vi.fn((url: string) => {
    throw new Error(`NEXT_REDIRECT:${url}`);
  }),
);

vi.mock("next/headers", () => ({
  cookies: cookiesMock,
}));

vi.mock("next/navigation", () => ({
  redirect: redirectMock,
}));

function mockCookies(token?: string) {
  cookiesMock.mockResolvedValue({
    get: vi.fn(() => (token ? { value: token } : undefined)),
  });
}

async function loadApiAuth(env: Record<string, string | undefined> = {}) {
  vi.resetModules();
  vi.stubEnv("APP_AUTH_PROVIDER", env.APP_AUTH_PROVIDER ?? "");
  vi.stubEnv("APP_API_TOKEN", env.APP_API_TOKEN ?? "");
  vi.stubEnv("APP_AUTH_BEARER_TOKEN", env.APP_AUTH_BEARER_TOKEN ?? "");
  vi.stubEnv("APP_AUTH_COOKIE_NAME", env.APP_AUTH_COOKIE_NAME ?? "");
  return import("./api-auth");
}

describe("requireAuth", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.unstubAllEnvs();
    mockCookies();
  });

  it("allows local no-auth actions when no auth provider or token is configured", async () => {
    const { requireAuth } = await loadApiAuth();

    await expect(requireAuth()).resolves.toBeUndefined();
  });

  it("allows server-token actions without a browser auth cookie", async () => {
    const { requireAuth } = await loadApiAuth({
      APP_API_TOKEN: "server-token",
    });

    await expect(requireAuth()).resolves.toBeUndefined();
  });

  it("redirects to sign-in when an auth provider is configured without credentials", async () => {
    const { requireAuth } = await loadApiAuth({ APP_AUTH_PROVIDER: "clerk" });

    await expect(requireAuth()).rejects.toThrow("NEXT_REDIRECT:/sign-in");
  });

  it("rejects an unknown auth provider instead of disabling authentication", async () => {
    await expect(async () => {
      const { requireAuth } = await loadApiAuth({
        APP_AUTH_PROVIDER: "unexpected",
      });
      await requireAuth();
    }).rejects.toThrow("Unsupported APP_AUTH_PROVIDER");
  });

  it("rejects a static API token combined with external user authentication", async () => {
    await expect(async () => {
      const { requireAuth } = await loadApiAuth({
        APP_AUTH_PROVIDER: "clerk",
        APP_API_TOKEN: "server-token",
      });
      await requireAuth();
    }).rejects.toThrow(
      "Static API tokens cannot be combined with external authentication",
    );
  });
});
