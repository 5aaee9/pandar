import { create } from "zustand";
import { persist } from "zustand/middleware";

import { defaultLocale, type Locale } from "../i18n/routing";

type Settings = {
  locale: Locale;
};

export const useSettings = create<Settings>()(
  persist(() => ({ locale: defaultLocale }), { name: "pandar.settings" }),
);
