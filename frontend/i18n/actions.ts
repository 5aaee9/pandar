"use server";

import { cookies } from "next/headers";

import { isLocale, type Locale } from "./routing";

export async function setLocale(locale: Locale): Promise<void> {
  if (!isLocale(locale)) {
    return;
  }
  const cookieStore = await cookies();
  cookieStore.set("locale", locale, {
    path: "/",
    maxAge: 60 * 60 * 24 * 365,
    sameSite: "lax",
    secure: process.env.NODE_ENV === "production",
  });
}
