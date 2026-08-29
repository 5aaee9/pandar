import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { LanguageSwitcher } from "../components/language-switcher";
import type { Tenant } from "./dashboard-types";

export function TicketSignInFrame({
  title,
  subtitle,
  meta,
  children,
}: {
  title: string;
  subtitle: string;
  meta: string;
  children: ReactNode;
}) {
  return (
    <main className="min-h-screen bg-background px-4 py-5 text-slate-950 sm:px-6 lg:px-8">
      <section className="mx-auto max-w-2xl overflow-hidden rounded-md border border-slate-300 bg-white">
        <div className="flex flex-col gap-3 border-b border-slate-200 px-4 py-3 sm:flex-row sm:items-start sm:justify-between">
          <div>
            <h1 className="text-base font-semibold">{title}</h1>
            <p className="mt-0.5 text-sm text-slate-600">{subtitle}</p>
          </div>
          <div className="flex items-center gap-2 self-start text-sm text-slate-600">
            <span>{meta}</span>
            <LanguageSwitcher />
          </div>
        </div>
        {children}
      </section>
    </main>
  );
}

export function TicketSignInEmptyState({
  actions,
  detail,
  detailLabel,
  message,
  statusLabel,
  title,
}: {
  actions: { href: string; label: string }[];
  detail?: string;
  detailLabel?: string;
  message: string;
  statusLabel: string;
  title: string;
}) {
  return (
    <div className="grid gap-5 px-4 py-10 text-center sm:px-6">
      <div>
        <div className="mb-2 inline-flex items-center rounded-md bg-red-50 px-2.5 py-1 text-xs font-semibold text-red-700">
          {statusLabel}
        </div>
        <h2 className="text-2xl font-semibold leading-8 text-slate-950">
          {title}
        </h2>
        <p className="mx-auto mt-2 max-w-md text-sm leading-6 text-slate-600">
          {message}
        </p>
      </div>
      <div className="flex flex-wrap justify-center gap-2">
        {actions.map((action, index) => (
          <a
            className={
              index === 0
                ? "inline-flex min-h-10 items-center rounded-md bg-primary px-3.5 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/80"
                : "inline-flex min-h-10 items-center rounded-md border border-slate-300 px-3.5 py-2 text-sm font-medium text-slate-700 hover:bg-slate-50"
            }
            href={action.href}
            key={`${action.href}-${action.label}`}
          >
            {action.label}
          </a>
        ))}
      </div>
      {detail ? (
        <details className="mx-auto w-full max-w-lg text-left text-xs text-slate-600">
          <summary className="cursor-pointer text-center font-medium text-slate-700">
            {detailLabel}
          </summary>
          <div className="mt-2 break-words rounded-md bg-slate-100 px-3 py-2 font-mono leading-5 text-slate-700">
            {detail}
          </div>
        </details>
      ) : null}
    </div>
  );
}

export function TenantTicketSelection({
  action,
  continueLabel,
  hiddenFields,
  selectionTitle,
  tenantLabel,
  tenants,
}: {
  action: string;
  continueLabel: string;
  hiddenFields: Record<string, string>;
  selectionTitle: string;
  tenantLabel: string;
  tenants: Tenant[];
}) {
  return (
    <div className="grid gap-3 px-4 py-4">
      <div className="text-sm font-semibold text-slate-950">{selectionTitle}</div>
      <form className="grid gap-3" action={action}>
        {Object.entries(hiddenFields).map(([name, value]) => (
          <input key={name} name={name} type="hidden" value={value} />
        ))}
        <label className="grid gap-1 text-sm">
          <span className="text-xs font-medium text-slate-500">
            {tenantLabel}
          </span>
          <select
            className="h-9 rounded-md border border-slate-300 bg-white px-2 text-sm text-slate-950 hover:border-slate-400"
            name="tenant"
          >
            {tenants.map((tenant) => (
              <option key={tenant.id} value={tenant.id}>
                {tenant.display_name}
              </option>
            ))}
          </select>
        </label>
        <Button size="lg" type="submit">
          {continueLabel}
        </Button>
      </form>
    </div>
  );
}
