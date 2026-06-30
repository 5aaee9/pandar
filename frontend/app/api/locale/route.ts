import { cookies } from "next/headers";

import { isLocale } from "../../../i18n/routing";

export async function POST(request: Request) {
  const body = (await request.json()) as { locale?: unknown };
  const locale = body.locale;
  if (typeof locale !== "string" || !isLocale(locale)) {
    return new Response(null, { status: 400 });
  }

  const cookieStore = await cookies();
  cookieStore.set("locale", locale, {
    path: "/",
    maxAge: 60 * 60 * 24 * 365,
    sameSite: "lax",
    secure: process.env.NODE_ENV === "production",
  });

  return new Response(null, { status: 204 });
}
