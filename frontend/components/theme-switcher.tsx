"use client";

import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";

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
          <Button
            aria-pressed={isActive}
            className={`rounded-md px-3 ${isActive ? "" : "text-muted-foreground"}`}
            key={theme}
            onClick={() => useSettings.setState({ theme })}
            size="sm"
            type="button"
            variant={isActive ? "default" : "ghost"}
          >
            {t(labelKeys[theme])}
          </Button>
        );
      })}
    </div>
  );
}
