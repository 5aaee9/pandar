import { create } from "zustand";
import { persist } from "zustand/middleware";

import { defaultLocale, type Locale } from "../i18n/routing";

export const themeModes = ["system", "light", "dark"] as const;
export type ThemeMode = (typeof themeModes)[number];

type Settings = {
  locale: Locale;
  theme: ThemeMode;
};

export const useSettings = create<Settings>()(
  persist<Settings>(() => ({ locale: defaultLocale, theme: "system" }), {
    name: "pandar.settings",
  }),
);
