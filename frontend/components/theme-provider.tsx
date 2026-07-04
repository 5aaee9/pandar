"use client";

import { useEffect, type ReactNode } from "react";

import { type ThemeMode, useSettings } from "../lib/settings-store";

const mediaQuery = "(prefers-color-scheme: dark)";

function resolvedTheme(theme: ThemeMode, prefersDark: boolean) {
  if (theme === "system") {
    return prefersDark ? "dark" : "light";
  }
  return theme;
}

function applyTheme(theme: ThemeMode, prefersDark: boolean) {
  const resolved = resolvedTheme(theme, prefersDark);
  const root = document.documentElement;
  root.classList.toggle("dark", resolved === "dark");
  root.style.colorScheme = resolved;
  root.dataset.theme = theme;
  root.dataset.resolvedTheme = resolved;
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const theme = useSettings((settings) => settings.theme);

  useEffect(() => {
    const media = window.matchMedia(mediaQuery);
    const apply = () => applyTheme(theme, media.matches);

    apply();
    if (theme !== "system") {
      return;
    }

    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);

  return children;
}
