import { waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const callbackUrl = "http://127.0.0.1:13618/callback";
const config = {
  webUrl: "https://web.example.test",
  hubUrl: "https://hub.example.test",
  callbackUrl,
  usingDefaultServer: false,
  usingDefaultWebServer: false,
  usingDefaultHubServer: false,
  configNonce: "nonce-1",
};

describe("plugin-local callback ownership", () => {
  beforeEach(() => {
    vi.resetModules();
    document.body.innerHTML = `
      <form id="target-form"><input id="web-url"><button type="submit">Save</button></form>
      <div id="notice-list"></div>
      <div id="status"></div>
      <a id="continue-link"></a>
    `;
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    delete (window as Window & { wx?: unknown }).wx;
  });

  it("uses the callback URL returned by plugin-local without re-probing Studio", async () => {
    const postMessage = vi.fn();
    Object.assign(window, { wx: { postMessage } });
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        if (url.endsWith("/.well-known/pandar")) {
          return new Response(JSON.stringify({ hubUrl: config.hubUrl }));
        }
        if (url === "/config" && init?.method === "POST") {
          return new Response(JSON.stringify(config));
        }
        if (url === "/config") {
          return new Response(JSON.stringify(config));
        }
        throw new Error(`Unexpected fetch: ${url}`);
      }),
    );

    await import("../plugin-local/src/app");

    await waitFor(() => {
      const href =
        document.querySelector<HTMLAnchorElement>("#continue-link")!.href;
      expect(new URL(href).searchParams.get("redirect_url")).toBe(callbackUrl);
    });
    expect(postMessage).not.toHaveBeenCalled();
  });
});
