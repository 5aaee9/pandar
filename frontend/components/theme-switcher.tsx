"use client";

import { useTranslations } from "next-intl";

import { themeModes, type ThemeMode, useSettings } from "../lib/settings-store";

const labelKeys: Record<ThemeMode, string> = {
  system: "themeSystem",
  light: "themeLight",
  dark: "themeDark",
};

export function ThemeSwitcher() {
  const t = useTranslations("dashboardShell");
  const active = useSettings((settings) => settings.theme);

  return (
    <div className="inline-flex items-center gap-1 rounded-md border border-border bg-background p-0.5">
      {themeModes.map((theme) => {
        const isActive = theme === active;
        return (
          <button
            key={theme}
            aria-pressed={isActive}
            className={`rounded px-2 py-0.5 text-xs font-medium transition-colors ${
              isActive
                ? "bg-primary text-primary-foreground"
                : "text-muted-foreground hover:bg-muted hover:text-foreground"
            }`}
            onClick={() => useSettings.setState({ theme })}
            type="button"
          >
            {t(labelKeys[theme])}
          </button>
        );
      })}
    </div>
  );
}
