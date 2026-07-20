import { useTranslations } from "next-intl";

export function SettingsStaticPanels({
  languageSwitcher,
  themeSwitcher,
}: {
  languageSwitcher: React.ReactNode;
  themeSwitcher: React.ReactNode;
}) {
  const t = useTranslations("dashboardShell");

  return (
    <>
      <section className="rounded-md border border-border bg-card px-4 py-3 transition-colors duration-150 ease-out hover:border-border/80">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h2 className="text-base font-semibold text-card-foreground">
              {t("languageTitle")}
            </h2>
            <p className="mt-0.5 text-sm text-muted-foreground">
              {t("languageDescription")}
            </p>
          </div>
          {languageSwitcher}
        </div>
      </section>
      <section className="rounded-md border border-border bg-card px-4 py-3 transition-colors duration-150 ease-out hover:border-border/80">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h2 className="text-base font-semibold text-card-foreground">
              {t("themeTitle")}
            </h2>
            <p className="mt-0.5 text-sm text-muted-foreground">
              {t("themeDescription")}
            </p>
          </div>
          {themeSwitcher}
        </div>
      </section>
    </>
  );
}
