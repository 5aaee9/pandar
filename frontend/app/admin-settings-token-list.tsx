import { KeyRoundIcon, ShieldCheckIcon, TriangleAlertIcon, XCircleIcon } from "lucide-react";
import { useLocale, useTranslations } from "next-intl";

import { FormattedDate } from "../components/formatted-date";
import { mutedBgClasses } from "../lib/utils";
import { useAdminDate } from "./admin-model";
import {
  CreateTenantTokenDialog,
  RevokeTenantTokenDialog,
  RotateTenantTokenForm,
} from "./admin-token-dialogs";
import type { TenantToken } from "./dashboard-types";
import { getRelativeTime } from "./dayjs-relative-time";

function TokenExpiration({ value, nowMs }: { value: string; nowMs: number }) {
  const t = useTranslations("admin");
  const locale = useLocale();
  const formatDate = useAdminDate();
  const relativeTime = getRelativeTime(value, nowMs, locale);

  if (!relativeTime) {
    return <>{t("expires", { date: formatDate(value) })}</>;
  }

  const label =
    relativeTime.timestampMs <= nowMs ? "expiredRelative" : "expiresRelative";
  return (
    <time dateTime={value} title={formatDate(value)}>
      {t(label, { relative: relativeTime.relative })}
    </time>
  );
}

type TokenStatus = "active" | "expired" | "revoked";

const TOKEN_STATUS_STYLES = {
  active: "border-success/40 bg-success/10 text-success",
  expired: "border-warning/50 bg-warning/10 text-warning",
  revoked: "border-border bg-muted text-muted-foreground",
} satisfies Record<TokenStatus, string>;

const TOKEN_STATUS_ICONS = {
  active: ShieldCheckIcon,
  expired: TriangleAlertIcon,
  revoked: XCircleIcon,
} satisfies Record<TokenStatus, typeof ShieldCheckIcon>;

const TOKEN_STATUS_ORDER: Record<TokenStatus, number> = {
  active: 0,
  expired: 1,
  revoked: 2,
};

function getTokenStatus(token: TenantToken, nowMs: number): TokenStatus {
  if (token.revoked_at) {
    return "revoked";
  }
  if (nowMs > 0 && token.expires_at && Date.parse(token.expires_at) <= nowMs) {
    return "expired";
  }
  return "active";
}

function TokenStatusBadge({ status }: { status: TokenStatus }) {
  const t = useTranslations("admin");
  const Icon = TOKEN_STATUS_ICONS[status];

  return (
    <span
      className={
        "inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-xs font-medium " +
        TOKEN_STATUS_STYLES[status]
      }
    >
      <Icon aria-hidden="true" className="size-3" />
      {t("tokenStatus." + status)}
    </span>
  );
}

function TokenScope({ scope }: { scope: string }) {
  const t = useTranslations("admin");
  const labels: Record<string, string> = {
    "*": t("scopeAll"),
    "agent:register": t("scopeAgentRegister"),
    "plugin:studio": t("scopePluginStudio"),
  };

  return (
    <span className="inline-flex rounded-md border border-border bg-muted/60 px-2 py-0.5 text-xs font-medium text-muted-foreground">
      {labels[scope] ?? scope}
    </span>
  );
}

function TokenLastUsed({
  value,
  nowMs,
}: {
  value: string | null;
  nowMs: number;
}) {
  const t = useTranslations("admin");
  const locale = useLocale();
  const formatDate = useAdminDate();
  if (!value) {
    return <>{t("lastUsedNever")}</>;
  }
  const relativeTime = getRelativeTime(value, nowMs, locale);
  return (
    <time dateTime={value} title={formatDate(value)}>
      {relativeTime?.relative ?? formatDate(value)}
    </time>
  );
}

function TokenFact({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="rounded-md border border-border bg-muted/20 px-3 py-2">
      <dt className="text-xs font-medium text-muted-foreground">
        {label}
      </dt>
      <dd className="mt-1 text-xs font-medium text-foreground">{children}</dd>
    </div>
  );
}

export function TenantTokensTable({
  tenantId,
  tokens,
  nowMs,
}: {
  tenantId: string;
  tokens: TenantToken[];
  nowMs: number;
}) {
  const t = useTranslations("admin");
  const sortedTokens = [...tokens].sort((left, right) => {
    const statusDifference =
      TOKEN_STATUS_ORDER[getTokenStatus(left, nowMs)] -
      TOKEN_STATUS_ORDER[getTokenStatus(right, nowMs)];
    return (
      statusDifference ||
      Date.parse(right.created_at) - Date.parse(left.created_at) ||
      right.id.localeCompare(left.id)
    );
  });
  const activeCount = sortedTokens.filter(
    (token) => getTokenStatus(token, nowMs) === "active",
  ).length;

  return (
    <div>
      <div className={`flex flex-col gap-4 border-b border-border ${mutedBgClasses} px-4 py-4 sm:flex-row sm:items-center sm:justify-between`}>
        <div className="flex items-center gap-2">
          <span className="flex size-8 items-center justify-center rounded-lg border border-border bg-background text-muted-foreground">
            <KeyRoundIcon className="size-4" />
          </span>
          <div>
            <h3 className="text-sm font-semibold text-foreground">
              {t("tenantTokens")}
            </h3>
            <p className="mt-0.5 text-xs text-muted-foreground">
              {t("tenantTokensDescription")}
            </p>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <span className="text-xs text-muted-foreground">
            {t("tenantTokensSummary", {
              active: activeCount,
              total: tokens.length,
            })}
          </span>
          <CreateTenantTokenDialog tenantId={tenantId} />
        </div>
      </div>
      {tokens.length === 0 ? (
        <div className="px-4 py-12 text-center">
          <div className="text-sm font-semibold text-foreground">
            {t("noTokensTitle")}
          </div>
          <p className="mx-auto mt-2 max-w-md text-sm text-muted-foreground">
            {t("noTokensMessage")}
          </p>
        </div>
      ) : (
        <div className="divide-y divide-border">
          {sortedTokens.map((token) => {
            const status = getTokenStatus(token, nowMs);
            return (
              <article
                key={token.id}
                className="grid gap-4 px-4 py-4 text-sm transition-colors hover:bg-muted/20 md:grid-cols-[minmax(0,1fr)_auto] md:items-start xl:grid-cols-[minmax(0,1fr)_minmax(28rem,0.9fr)_auto] xl:items-center"
                data-token-id={token.id}
                data-token-status={status}
              >
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <div className="font-semibold text-foreground">
                      {token.name}
                    </div>
                    <TokenStatusBadge status={status} />
                  </div>
                  <div className="mt-2 flex flex-wrap gap-1">
                    {token.scopes.length === 0 ? (
                      <span className="inline-flex rounded-md border border-border bg-muted/60 px-2 py-0.5 text-xs font-medium text-muted-foreground">
                        {t("scopeReadOnly")}
                      </span>
                    ) : (
                      token.scopes.map((scope) => (
                        <TokenScope key={scope} scope={scope} />
                      ))
                    )}
                  </div>
                  <div className="mt-2 text-xs text-muted-foreground">
                    {t("tokenIdLabel")} {" "}
                    <code className="break-all font-mono" title={token.id}>
                      {token.id}
                    </code>
                  </div>
                </div>
                <dl className="grid gap-2 sm:grid-cols-3 md:col-span-2 xl:col-span-1 xl:col-start-2 xl:row-start-1">
                  <TokenFact label={t("tokenExpirationLabel")}>
                    {token.expires_at ? (
                      <TokenExpiration value={token.expires_at} nowMs={nowMs} />
                    ) : (
                      t("expiresNever")
                    )}
                  </TokenFact>
                  <TokenFact label={t("tokenLastUsedLabel")}>
                    <TokenLastUsed value={token.last_used_at} nowMs={nowMs} />
                  </TokenFact>
                  <TokenFact label={t("tokenCreatedLabel")}>
                    <FormattedDate value={token.created_at} />
                  </TokenFact>
                </dl>
                <div className="flex w-full flex-wrap items-start gap-2 md:col-start-2 md:row-start-1 md:w-auto md:justify-end xl:col-start-3">
                  <RotateTenantTokenForm
                    tenantId={tenantId}
                    token={token}
                    status={status}
                  />
                  {status !== "revoked" ? (
                    <RevokeTenantTokenDialog
                      tenantId={tenantId}
                      token={token}
                    />
                  ) : null}
                </div>
              </article>
            );
          })}
        </div>
      )}
    </div>
  );
}
