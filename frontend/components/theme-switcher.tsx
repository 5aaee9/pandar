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
    <div
      aria-label={t("themeTitle")}
      className="inline-flex items-center gap-1 rounded-lg border border-border bg-background p-1"
      role="group"
    >
      {themeModes.map((theme) => {
        const isActive = theme === active;
        return (
          <button
            key={theme}
            aria-pressed={isActive}
            className={`h-7 rounded-md px-3 text-xs font-medium transition-colors ${
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
