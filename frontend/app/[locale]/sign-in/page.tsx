import { cookies } from "next/headers";
import { notFound } from "next/navigation";

import { isLocale } from "../../../i18n/routing";
import PluginSignInPage from "../../plugin-sign-in/page";

export default async function LocaleSignInPage({
  params,
}: {
  params: Promise<{ locale: string }>;
}) {
  const { locale } = await params;
  if (!isLocale(locale)) {
    notFound();
  }
  const cookieStore = await cookies();
  if (cookieStore.get("locale")?.value !== locale) {
    cookieStore.set("locale", locale, {
      path: "/",
      maxAge: 60 * 60 * 24 * 365,
      sameSite: "lax",
      secure: process.env.NODE_ENV === "production",
    });
  }
  return <PluginSignInPage />;
}
