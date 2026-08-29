"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";

import type { Tenant } from "../dashboard-types";

type PluginTicketFormProps = {
  action: (formData: FormData) => Promise<void>;
  autoSelectedTenant: boolean;
  redirectUrl: string;
  selectedTenant: Tenant;
};

export function PluginTicketForm({
  action,
  autoSelectedTenant,
  redirectUrl,
  selectedTenant,
}: PluginTicketFormProps) {
  const t = useTranslations("signIn");
  const [customCallbackUrl, setCustomCallbackUrl] = useState<string | null>(
    null,
  );
  const callbackUrl = customCallbackUrl ?? redirectUrl;

  return (
    <form action={action} className="grid gap-4 px-4 py-4">
      <input name="tenant_id" type="hidden" value={selectedTenant.id} />
      <label className="grid gap-1 text-sm">
        <span className="flex items-center justify-between gap-2">
          <span className="text-xs font-medium text-slate-600">
            {t("callbackUrl")}
          </span>
          <span className="rounded-md border border-slate-200 bg-slate-50 px-2 py-0.5 text-xs font-medium text-slate-600">
            {t("callbackDefault")}
          </span>
        </span>
        <input
          className="h-9 rounded-md border border-slate-300 px-2 font-mono text-xs text-slate-950 hover:border-slate-400"
          name="redirect_url"
          onChange={(event) => {
            setCustomCallbackUrl(event.currentTarget.value);
          }}
          required
          type="url"
          value={callbackUrl}
        />
        <span className="text-xs leading-5 text-slate-600">
          {t("callbackHelp")}
        </span>
      </label>
      <div className="grid gap-2 rounded-md border border-slate-200 bg-slate-50 px-3 py-2 text-sm">
        {autoSelectedTenant ? (
          <div className="rounded-md bg-white px-2 py-1.5 text-xs leading-5 text-slate-700">
            {t("singleTenantReady")}
          </div>
        ) : null}
        <div className="flex items-center justify-between gap-3">
          <span className="text-xs font-medium text-slate-600">
            {t("tenant")}
          </span>
          <span className="font-medium text-slate-950">
            {selectedTenant.display_name}
          </span>
        </div>
        <div className="flex items-center justify-between gap-3">
          <span className="text-xs font-medium text-slate-600">
            {t("tenantId")}
          </span>
          <span className="break-all text-right font-mono text-xs text-slate-700">
            {selectedTenant.id}
          </span>
        </div>
        <div className="border-t border-slate-200 pt-2 text-xs leading-5 text-slate-600">
          {t("ticketSummary")}
        </div>
      </div>
      <Button size="lg" type="submit">
        {t("signInSubmit")}
      </Button>
    </form>
  );
}
