import "./globals.css";

import type { Metadata } from "next";
import type { ReactNode } from "react";

import { getAuthLocale } from "../lib/i18n";

export const metadata: Metadata = {
  title: "Pandar Auth",
  description: "Self-hosted Pandar authentication issuer",
};

export default async function RootLayout({
  children,
}: {
  children: ReactNode;
}) {
  const locale = await getAuthLocale();

  return (
    <html lang={locale}>
      <body>{children}</body>
    </html>
  );
}
