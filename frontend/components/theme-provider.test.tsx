import { render } from "@testing-library/react";
import Script from "next/script";
import { describe, expect, it } from "vitest";

import { ThemeProvider } from "./theme-provider";
import { ThemeScript } from "./theme-script";

describe("theme components", () => {
  it("does not render the bootstrap script from the client provider", () => {
    const { container } = render(
      <ThemeProvider>
        <div>content</div>
      </ThemeProvider>,
    );

    expect(container.querySelector("script")).toBeNull();
  });

  it("registers the bootstrap as a before-interactive Next.js script", () => {
    const element = ThemeScript();

    expect(element.type).toBe(Script);
    expect(element.props).toMatchObject({
      id: "pandar-theme",
      strategy: "beforeInteractive",
    });
    expect(element.props.dangerouslySetInnerHTML.__html).toContain(
      "pandar.settings",
    );
  });
});
