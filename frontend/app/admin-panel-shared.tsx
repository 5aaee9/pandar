'use client'

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { CheckIcon, CopyIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { inputClasses, mutedBgSubtleClasses } from "../lib/utils";
import type { SecretActionState } from "./action-state";

function CopyButton({ label, value }: { label: string; value: string }) {
  const t = useTranslations("admin");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) {
      return;
    }
    const timeout = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(timeout);
  }, [copied]);

  return (
    <Button
      className="self-start"
      onClick={async () => {
        if (!navigator.clipboard) {
          return;
        }
        try {
          await navigator.clipboard.writeText(value);
          setCopied(true);
        } catch {
          setCopied(false);
        }
      }}
      size="xs"
      type="button"
      variant="outline"
    >
      {copied ? (
        <CheckIcon aria-hidden="true" className="size-3.5 text-success" />
      ) : (
        <CopyIcon aria-hidden="true" className="size-3.5" />
      )}
      {copied ? t("copied") : label}
    </Button>
  );
}

const RESULT_CLASS =
  "grid gap-1.5 rounded-md border border-warning/50 bg-warning/10 px-2 py-2 text-xs text-foreground";
const SECRET_CLASS =
  "break-all rounded border border-border bg-background px-2 py-1 font-mono text-[11px] text-foreground";

export function SecretActionResult({ state }: { state: SecretActionState }) {
  const t = useTranslations("admin");
  if (!state) {
    return null;
  }
  if (!state.ok) {
    return (
      <div
        className="rounded-md border border-destructive/30 bg-destructive/10 px-2 py-1 text-xs text-destructive"
        role="alert"
      >
        {state.error}
      </div>
    );
  }
  if (state.kind === "tenant_token") {
    return (
      <div data-motion="secret-result" className={RESULT_CLASS} role="status">
        <div className="font-semibold">
          {t(state.operation === "created" ? "tokenCreated" : "tokenRotated")}
        </div>
        <code className={SECRET_CLASS}>{state.token}</code>
        <div>{t("tokenShownOnce")}</div>
        <CopyButton label={t("copyToken")} value={state.token} />
      </div>
    );
  }
  if (state.kind === "join_link") {
    const joinUrl =
      typeof window === "undefined"
        ? `/join#${state.token}`
        : `${window.location.origin}/join#${state.token}`;
    return (
      <div data-motion="secret-result" className={RESULT_CLASS} role="status">
        <div className="font-semibold">{state.message}</div>
        <code className={SECRET_CLASS}>{joinUrl}</code>
        <div>{t("joinTokenShownOnce")}</div>
        <CopyButton label={t("copyJoinLink")} value={joinUrl} />
      </div>
    );
  }
  return (
    <div data-motion="secret-result" className={RESULT_CLASS} role="status">
      <div className="font-semibold">{state.message}</div>
      <pre className="overflow-x-auto rounded border border-border bg-background px-2 py-1 font-mono text-[11px] text-foreground">
        {state.agentEnv}
      </pre>
      <div>{t("pairingShownOnce")}</div>
      <CopyButton label={t("copyAgentEnv")} value={state.agentEnv} />
    </div>
  );
}

function FormField({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="grid gap-1 text-sm">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      {children}
    </label>
  );
}

export function Input({
  name,
  label,
  defaultValue,
  disabled = false,
  describedBy,
  min,
  placeholder,
  required = false,
  type = "text",
}: {
  name: string;
  label: string;
  defaultValue?: string;
  disabled?: boolean;
  describedBy?: string;
  min?: string;
  placeholder?: string;
  required?: boolean;
  type?: string;
}) {
  return (
    <FormField label={label}>
      <input
        aria-describedby={describedBy}
        className={inputClasses}
        defaultValue={defaultValue}
        disabled={disabled}
        min={min}
        name={name}
        placeholder={placeholder}
        required={required}
        type={type}
      />
    </FormField>
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
    <Button className="h-9 px-3" disabled={disabled} size="default" type="submit">
      {label}
    </Button>
  );
}

export function Subhead({ title, meta }: { title: string; meta: string }) {
  return (
    <div className={`flex items-center justify-between border-b border-border ${mutedBgSubtleClasses} px-4 py-2`}>
      <h3 className="text-sm font-semibold text-foreground">{title}</h3>
      <span className="text-xs text-muted-foreground">{meta}</span>
    </div>
  );
}
