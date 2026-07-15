import { act } from "react";
import { hydrateRoot } from "react-dom/client";
import { renderToString } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useIsMobile } from "./use-mobile";

function ResponsiveProbe() {
  return useIsMobile() ? (
    <main data-layout="mobile">mobile</main>
  ) : (
    <div data-layout="desktop">desktop</div>
  );
}

describe("useIsMobile", () => {
  afterEach(() => {
    document.body.innerHTML = "";
    vi.unstubAllGlobals();
  });

  it("hydrates the server layout before switching to the mobile layout", async () => {
    const browserWindow = window;
    const originalInnerWidth = window.innerWidth;
    const container = document.createElement("div");
    document.body.appendChild(container);

    vi.stubGlobal("window", undefined);
    container.innerHTML = renderToString(<ResponsiveProbe />);
    vi.stubGlobal("window", browserWindow);
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      value: 375,
    });

    const recoverableErrors: unknown[] = [];
    let root: ReturnType<typeof hydrateRoot> | undefined;
    await act(async () => {
      root = hydrateRoot(container, <ResponsiveProbe />, {
        onRecoverableError: (error) => recoverableErrors.push(error),
      });
    });

    expect(recoverableErrors).toEqual([]);
    expect(container.querySelector('[data-layout="mobile"]')).not.toBeNull();

    await act(async () => root?.unmount());
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      value: originalInnerWidth,
    });
  });
});
