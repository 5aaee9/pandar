import { render } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
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

  it("keeps the bootstrap script server-renderable", () => {
    const markup = renderToStaticMarkup(<ThemeScript />);

    expect(markup).toContain("<script");
    expect(markup).toContain("pandar.settings");
  });
});
