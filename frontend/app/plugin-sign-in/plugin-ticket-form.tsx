"use client";

import { useEffect, useState } from "react";

import type { Tenant } from "../dashboard-types";

type PluginTicketFormProps = {
  action: (formData: FormData) => Promise<void>;
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
  redirectUrl,
  selectedTenant,
}: PluginTicketFormProps) {
  const [callbackUrl, setCallbackUrl] = useState(redirectUrl);

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
        setCallbackUrl(data.response.base_url);
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

  return (
    <form action={action} className="grid gap-3 px-4 py-4">
      <input name="tenant_id" type="hidden" value={selectedTenant.id} />
      <label className="grid gap-1 text-sm">
        <span className="text-xs font-medium text-slate-500">
          Local callback URL
        </span>
        <input
          className="h-9 rounded-md border border-slate-300 px-2 text-sm text-slate-950"
          name="redirect_url"
          onChange={(event) => setCallbackUrl(event.currentTarget.value)}
          required
          value={callbackUrl}
        />
      </label>
      <div className="rounded-md border border-slate-200 bg-slate-50 px-3 py-2 text-sm text-slate-700">
        {selectedTenant.display_name}
      </div>
      <button
        className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white"
        type="submit"
      >
        Sign in to Studio
      </button>
    </form>
  );
}
