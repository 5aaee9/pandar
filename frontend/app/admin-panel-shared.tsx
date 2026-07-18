import { useTranslations } from "next-intl";

import type { SecretActionState } from "./action-state";

export function SecretActionResult({ state }: { state: SecretActionState }) {
  const t = useTranslations("admin");
  if (!state) {
    return null;
  }
  if (!state.ok) {
    return (
      <div className="rounded-md border border-destructive/30 bg-destructive/10 px-2 py-1 text-xs text-destructive">
        {state.error}
      </div>
    );
  }
  if (state.kind === "tenant_token") {
    return (
      <div data-motion="secret-result" className="grid gap-1 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-2 text-xs text-foreground">
        <div className="font-semibold">
          {t(state.operation === "created" ? "tokenCreated" : "tokenRotated")}
        </div>
        <code className="break-all rounded border border-border bg-background px-2 py-1 font-mono text-[11px] text-foreground">
          {state.token}
        </code>
        <div>{t("tokenShownOnce")}</div>
      </div>
    );
  }
  if (state.kind === "join_link") {
    return (
      <div data-motion="secret-result" className="grid gap-1 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-2 text-xs text-foreground">
        <div className="font-semibold">{state.message}</div>
        <code className="break-all rounded border border-border bg-background px-2 py-1 font-mono text-[11px] text-foreground">{`/join#${state.token}`}</code>
        <div>{t("joinTokenShownOnce")}</div>
      </div>
    );
  }
  return (
    <div data-motion="secret-result" className="grid gap-1 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-2 text-xs text-foreground">
      <div className="font-semibold">{state.message}</div>
      <pre className="overflow-x-auto rounded border border-border bg-background px-2 py-1 font-mono text-[11px] text-foreground">
        {state.agentEnv}
      </pre>
      <div>{t("pairingShownOnce")}</div>
    </div>
  );
}

export function Input({
  name,
  label,
  defaultValue,
  disabled = false,
  placeholder,
  type = "text",
}: {
  name: string;
  label: string;
  defaultValue?: string;
  disabled?: boolean;
  placeholder?: string;
  type?: string;
}) {
  return (
    <label className="grid gap-1 text-sm">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      <input
        className="h-9 rounded-md border border-input bg-background px-2 text-sm text-foreground shadow-xs outline-none transition-[color,box-shadow] placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-60"
        defaultValue={defaultValue}
        disabled={disabled}
        name={name}
        placeholder={placeholder}
        type={type}
      />
    </label>
  );
}

export function Select({
  name,
  label,
  values,
}: {
  name: string;
  label: string;
  values: string[];
}) {
  return (
    <label className="grid gap-1 text-sm">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      <select
        className="h-9 rounded-md border border-input bg-background px-2 text-sm text-foreground shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50"
        name={name}
      >
        {values.map((value) => (
          <option key={value} value={value}>
            {value}
          </option>
        ))}
      </select>
    </label>
  );
}

export function PrimaryButton({
  label,
  disabled = false,
}: {
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      className="h-9 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground hover:bg-primary/80 disabled:cursor-not-allowed disabled:opacity-50"
      disabled={disabled}
      type="submit"
    >
      {label}
    </button>
  );
}

export function Subhead({ title, meta }: { title: string; meta: string }) {
  return (
    <div className="flex items-center justify-between border-b border-border bg-muted/30 px-4 py-2">
      <h3 className="text-sm font-semibold text-foreground">{title}</h3>
      <span className="text-xs text-muted-foreground">{meta}</span>
    </div>
  );
}
