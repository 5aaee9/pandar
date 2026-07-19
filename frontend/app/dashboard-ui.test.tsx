import { render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { describe, expect, it } from "vitest";

import en from "../messages/en.json";
import zh from "../messages/zh.json";
import { StatusBadge } from "./dashboard-ui";

describe("StatusBadge", () => {
  it.each([
    ["en", "PREPARE", "Preparing"],
    ["en", "SLICING", "Slicing"],
    ["en", "PAUSE", "Paused"],
    ["en", "PAUSED", "Paused"],
    ["en", "FINISH", "Finished"],
    ["zh", "PREPARE", "准备中"],
    ["zh", "SLICING", "切片中"],
    ["zh", "PAUSE", "已暂停"],
    ["zh", "PAUSED", "已暂停"],
    ["zh", "FINISH", "已完成"],
  ] as const)(
    "localizes the %s %s printer status as %s",
    (locale, status, label) => {
      render(
        <NextIntlClientProvider
          locale={locale}
          messages={locale === "zh" ? zh : en}
        >
          <StatusBadge value={status} />
        </NextIntlClientProvider>,
      );

      expect(screen.getByText(label)).toBeVisible();
      expect(screen.queryByText(status)).not.toBeInTheDocument();
    },
  );
});
