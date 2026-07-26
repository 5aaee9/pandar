import Script from "next/script";

const mediaQuery = "(prefers-color-scheme: dark)";

export function ThemeScript({ nonce }: { nonce?: string } = {}) {
  const code = `(() => {
  try {
    const raw = localStorage.getItem("pandar.settings");
    const stored = raw ? JSON.parse(raw)?.state?.theme : "system";
    const theme = stored === "light" || stored === "dark" ? stored : "system";
    const prefersDark = window.matchMedia("${mediaQuery}").matches;
    const resolved = theme === "system" ? (prefersDark ? "dark" : "light") : theme;
    const root = document.documentElement;
    root.classList.toggle("dark", resolved === "dark");
    root.style.colorScheme = resolved;
    root.dataset.theme = theme;
    root.dataset.resolvedTheme = resolved;
  } catch {}
})();`;

  return (
    <Script
      id="pandar-theme"
      nonce={nonce}
      strategy="beforeInteractive"
      dangerouslySetInnerHTML={{ __html: code }}
    />
  );
}
