import enCatalog from "./hms-catalog-data/en.json";
import zhCnCatalog from "./hms-catalog-data/zh-cn.json";

type HmsCatalog = {
  prefixes: string[];
  messages: string[];
  codes: Record<string, Array<number | null>>;
};

const catalogs: Record<"en" | "zh-cn", HmsCatalog> = {
  en: enCatalog,
  "zh-cn": zhCnCatalog,
};

export function hmsMessage(serialNumber: string, code: string, locale: string) {
  const catalog =
    catalogs[locale.toLowerCase().startsWith("zh") ? "zh-cn" : "en"];
  const prefixIndex = catalog.prefixes.indexOf(
    serialNumber.slice(0, 3).toUpperCase(),
  );
  if (prefixIndex === -1) {
    return null;
  }

  const messageIndex = catalog.codes[code.toUpperCase()]?.[prefixIndex];
  return typeof messageIndex === "number"
    ? catalog.messages[messageIndex]
    : null;
}
