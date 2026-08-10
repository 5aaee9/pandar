import { afterEach, describe, expect, it, vi } from "vitest";

describe("Pandar Web discovery", () => {
  afterEach(() => {
    vi.resetModules();
    vi.unstubAllEnvs();
  });

  it("publishes the Hub URL for plugin discovery", async () => {
    vi.stubEnv("APP_API_URL", "http://pandar-api:8080");
    vi.stubEnv("APP_PUBLIC_API_URL", "https://hub.example.test/");
    const { GET } = await import("./route");

    const response = await GET();

    expect(response.status).toBe(200);
    await expect(response.json()).resolves.toEqual({
      hubUrl: "https://hub.example.test",
    });
    expect(response.headers.get("access-control-allow-origin")).toBe("*");
  });
});
