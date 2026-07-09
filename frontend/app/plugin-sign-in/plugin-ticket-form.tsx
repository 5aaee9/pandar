"use client";

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";

import type { Tenant } from "../dashboard-types";

type PluginTicketFormProps = {
  action: (formData: FormData) => Promise<void>;
  autoSelectedTenant: boolean;
  redirectUrl: string;
  selectedTenant: Tenant;
};

type StudioWindow = Window & {
  wx?: {
    postMessage?: (message: string) => void;
  };
};

type StudioLocalhostMessage = {
  command?: string;
  response?: {
    base_url?: string;
  };
  sequence_id?: string;
};

export function PluginTicketForm({
  action,
  autoSelectedTenant,
  redirectUrl,
  selectedTenant,
}: PluginTicketFormProps) {
  const t = useTranslations("signIn");
  const [studioCallbackUrl, setStudioCallbackUrl] = useState<string | null>(
    null,
  );
  const [customCallbackUrl, setCustomCallbackUrl] = useState<string | null>(
    null,
  );
  const [callbackSource, setCallbackSource] = useState<"default" | "studio">(
    "default",
  );

  useEffect(() => {
    const studioWindow = window as StudioWindow;
    const sequenceId = `pandar-${Date.now()}-${Math.random().toString(36).slice(2)}`;

    function handleMessage(event: MessageEvent) {
      let data: StudioLocalhostMessage;
      try {
        data =
          typeof event.data === "string" ? JSON.parse(event.data) : event.data;
      } catch {
        return;
      }
      if (!data || typeof data !== "object") {
        return;
      }
      if (
        data.command === "get_localhost_url" &&
        data.sequence_id === sequenceId &&
        data.response?.base_url
      ) {
        setStudioCallbackUrl(data.response.base_url);
        setCustomCallbackUrl(null);
        setCallbackSource("studio");
      }
    }

    window.addEventListener("message", handleMessage);
    studioWindow.wx?.postMessage?.(
      JSON.stringify({
        command: "get_localhost_url",
        sequence_id: sequenceId,
      }),
    );
    return () => window.removeEventListener("message", handleMessage);
  }, []);

  const callbackSourceLabel =
    callbackSource === "studio" ? t("callbackDetected") : t("callbackDefault");
  const callbackUrl = customCallbackUrl ?? studioCallbackUrl ?? redirectUrl;

  return (
    <form action={action} className="grid gap-4 px-4 py-4">
      <input name="tenant_id" type="hidden" value={selectedTenant.id} />
      <label className="grid gap-1 text-sm">
        <span className="flex items-center justify-between gap-2">
          <span className="text-xs font-medium text-slate-600">
            {t("callbackUrl")}
          </span>
          <span className="rounded-md border border-slate-200 bg-slate-50 px-2 py-0.5 text-xs font-medium text-slate-600">
            {callbackSourceLabel}
          </span>
        </span>
        <input
          className="h-9 rounded-md border border-slate-300 px-2 font-mono text-xs text-slate-950 hover:border-slate-400"
          name="redirect_url"
          onChange={(event) => {
            setCustomCallbackUrl(event.currentTarget.value);
            setCallbackSource("default");
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
      <button
        className="h-9 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground hover:bg-primary/80"
        type="submit"
      >
        {t("signInSubmit")}
      </button>
    </form>
  );
}
