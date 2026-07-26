import { getTranslations } from "next-intl/server";

import type { Tenant } from "../dashboard-types";

type MobileTicketFormProps = {
  action: (formData: FormData) => Promise<void>;
  autoSelectedTenant: boolean;
  codeChallenge: string;
  redirectUrl: string;
  selectedTenant: Tenant;
  state: string;
};

export async function MobileTicketForm({
  action,
  autoSelectedTenant,
  codeChallenge,
  redirectUrl,
  selectedTenant,
  state,
}: MobileTicketFormProps) {
  const t = await getTranslations("signIn");

  return (
    <form action={action} className="grid gap-4 px-4 py-4">
      <input name="tenant_id" type="hidden" value={selectedTenant.id} />
      <input name="redirect_url" type="hidden" value={redirectUrl} />
      <input name="code_challenge" type="hidden" value={codeChallenge} />
      <input name="state" type="hidden" value={state} />
      <div className="grid gap-2 rounded-md border border-slate-200 bg-slate-50 px-3 py-2 text-sm">
        {autoSelectedTenant ? (
          <div className="rounded-md bg-white px-2 py-1.5 text-xs leading-5 text-slate-700">
            {t("mobileSingleTenantReady")}
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
          {t("mobileTicketSummary")}
        </div>
      </div>
      <button
        className="h-9 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground hover:bg-primary/80"
        type="submit"
      >
        {t("mobileSignInSubmit")}
      </button>
    </form>
  );
}
