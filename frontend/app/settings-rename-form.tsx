"use client";

import { useActionState, useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { Loader2Icon } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { inputClasses } from "@/lib/utils";
import { updateTenantDisplayName } from "./admin-actions";
import type { Tenant } from "./dashboard-types";

export function WorkspaceRenameForm({ tenant }: { tenant: Tenant }) {
  const t = useTranslations("settingsPage");
  const [state, formAction, pending] = useActionState(
    updateTenantDisplayName,
    null,
  );
  const [value, setValue] = useState(tenant.display_name);

  useEffect(() => {
    if (state?.ok) {
      toast.success(t("renameSuccess"));
    }
  }, [state, t]);

  const dirty = value.trim().length > 0 && value.trim() !== tenant.display_name;

  return (
    <form
      action={formAction}
      className="border-b border-border px-4 py-4 sm:px-5"
      key={tenant.display_name}
    >
      <input name="tenant_id" type="hidden" value={tenant.id} />
      <label
        className="text-sm font-medium text-foreground"
        htmlFor="workspace-display-name"
      >
        {t("workspaceName")}
      </label>
      <p className="mt-0.5 text-sm text-muted-foreground">
        {t("renameDescription")}
      </p>
      <div className="mt-3 flex flex-col gap-2 sm:flex-row sm:items-center">
        <input
          className={`${inputClasses} sm:max-w-xs`}
          id="workspace-display-name"
          name="display_name"
          onChange={(event) => setValue(event.target.value)}
          required
          type="text"
          value={value}
        />
        <Button disabled={pending || !dirty} type="submit">
          {pending ? <Loader2Icon className="animate-spin" /> : null}
          {pending ? t("renameSaving") : t("renameSave")}
        </Button>
      </div>
      {state && !state.ok ? (
        <p className="mt-2 text-sm text-destructive" role="alert">
          {t.has(`errors.${state.error}`)
            ? t(`errors.${state.error}`)
            : t("renameError")}
        </p>
      ) : null}
    </form>
  );
}
