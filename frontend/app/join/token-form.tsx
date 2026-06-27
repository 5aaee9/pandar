"use client";

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";

export function JoinTokenForm({
  action,
}: {
  action: (formData: FormData) => void;
}) {
  const t = useTranslations("join");
  const [token, setToken] = useState("");

  useEffect(() => {
    setToken(window.location.hash.slice(1));
  }, []);

  return (
    <form action={action} className="grid gap-3 px-4 py-4">
      <label className="grid gap-1 text-sm">
        <span className="text-xs font-medium text-slate-500">
          {t("joinToken")}
        </span>
        <input
          className="h-9 rounded-md border border-slate-300 px-2 font-mono text-sm text-slate-950"
          name="token"
          onChange={(event) => setToken(event.target.value)}
          required
          value={token}
        />
      </label>
      <button
        className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white"
        type="submit"
      >
        {t("joinSubmit")}
      </button>
    </form>
  );
}
