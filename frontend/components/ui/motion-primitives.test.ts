import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

const root = dirname(dirname(dirname(fileURLToPath(import.meta.url))));

async function readSource(path: string): Promise<string> {
  return readFile(join(root, path), "utf8");
}

describe("shared motion primitives", () => {
  it("uses state-driven semantic motion classes instead of keyframes", async () => {
    const primitives = [
      ["components/ui/dialog.tsx", "pandar-dialog-content-motion"],
      ["components/ui/popover.tsx", "pandar-popover-motion"],
      ["components/ui/hover-card.tsx", "pandar-hover-card-motion"],
      ["components/ui/tooltip.tsx", "pandar-tooltip-motion"],
      ["components/ui/sheet.tsx", "pandar-sheet-content-motion"],
    ] as const;

    for (const [path, semanticClass] of primitives) {
      const source = await readSource(path);
      expect(source).toContain(semanticClass);
      expect(source).not.toMatch(
        /animate-in|animate-out|slide-in|zoom-in|zoom-out/,
      );
    }
  });

  it("keeps dialog centering while removing decorative reduced motion", async () => {
    const css = await readSource("app/globals.css");
    const reducedMotion = css.slice(
      css.indexOf("@media (prefers-reduced-motion: reduce)"),
      css.indexOf("@theme inline"),
    );

    expect(reducedMotion).toMatch(
      /\.pandar-dialog-content-motion\[data-starting-style\],[^}]*scale: 1;/s,
    );
    expect(reducedMotion).not.toMatch(
      /\.pandar-dialog-content-motion\[data-starting-style\],[^}]*translate/s,
    );
    expect(reducedMotion).toMatch(
      /\.pandar-popover-motion\[data-side\]\[data-starting-style\],[^}]*translate: none;[^}]*scale: 1;/s,
    );
    expect(reducedMotion).toMatch(
      /\.pandar-sheet-content-motion\[data-side\]\[data-starting-style\],[^}]*translate: none;/s,
    );
  });

  it("defines the shared motion vocabulary exactly", async () => {
    const css = await readSource("app/globals.css");

    expect(css).toContain("--ease-out: cubic-bezier(0.23, 1, 0.32, 1);");
    expect(css).toContain("--ease-in-out: cubic-bezier(0.77, 0, 0.175, 1);");
    expect(css).toContain("--ease-drawer: cubic-bezier(0.32, 0.72, 0, 1);");
    expect(css).toContain("--motion-duration-feedback: 150ms;");
    expect(css).toContain("--motion-duration-modal: 200ms;");
    expect(css).toContain("--motion-duration-drawer: 200ms;");
  });
});
