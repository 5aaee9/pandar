import { getRequestConfig } from "next-intl/server";
import { headers, cookies } from "next/headers";

import enMessages from "../messages/en.json";
import zhMessages from "../messages/zh.json";
import { defaultLocale, isLocale, type Locale } from "./routing";

const messagesByLocale = {
  en: enMessages,
  zh: zhMessages,
} satisfies Record<Locale, unknown>;

export default getRequestConfig(async () => {
  const [cookieStore, headerList] = await Promise.all([cookies(), headers()]);
  const cookieLocale = cookieStore.get("locale")?.value;
  const acceptLanguage = headerList.get("accept-language") ?? "";
  const locale: Locale = resolveLocale(cookieLocale, acceptLanguage);
  return {
    locale,
    messages: messagesByLocale[locale],
  };
});

function resolveLocale(
  cookie: string | undefined,
  acceptLanguage: string,
): Locale {
  if (isLocale(cookie)) {
    return cookie;
  }
  if (/\bzh(?:\b|[-_])/i.test(acceptLanguage)) {
    return "zh";
  }
  return defaultLocale;
}
