"use client";

import { useActionState, useState } from "react";
import { useTranslations } from "next-intl";
import { TrashIcon } from "lucide-react";

import { removeTenantUser, updateTenantUserRole } from "./admin-actions";
import { roles, useAdminDate } from "./admin-model";
import { ConfirmForm } from "./confirm-dialog";
import { Tag } from "./dashboard-ui";
import type { Tenant, User, UserIdentity } from "./dashboard-types";
import { inputSmClasses, monoIdClasses } from "../lib/utils";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { isLastTenantAdmin, isSelf } from "./users-model";
import { useMutationFeedback } from "./mutation-feedback";
import {
  invalidateTenantResources,
  mutationResources,
} from "./mutation-invalidation";
import { useQueryClient } from "@tanstack/react-query";
import { UserAvatar, YouBadge } from "./users-shared";

export function MemberDialog({
  tenant,
  user,
  users,
  identities,
  meEmail,
  open,
  onOpenChange,
}: {
  tenant: Tenant;
  user: User;
  users: User[];
  identities: UserIdentity[];
  meEmail: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const t = useTranslations("usersPage");
  const formatDate = useAdminDate();
  const self = isSelf(user, meEmail);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <div className="flex items-center gap-3">
            <UserAvatar name={user.display_name} size="lg" />
            <div className="min-w-0">
              <DialogTitle className="flex items-center gap-2">
                <span className="truncate">{user.display_name}</span>
                {self ? <YouBadge /> : null}
              </DialogTitle>
              <DialogDescription className="truncate">
                {user.email}
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>
        <div className="grid gap-5">
          <div className="grid gap-1 text-sm">
            <div className="flex items-center justify-between gap-2">
              <span className="text-muted-foreground">{t("joinedLabel")}</span>
              <span className="text-foreground">
                {formatDate(user.created_at)}
              </span>
            </div>
            <div className="flex items-center justify-between gap-2">
              <span className="text-muted-foreground">{t("idLabel")}</span>
              <span className={monoIdClasses}>{user.id}</span>
            </div>
          </div>
          <RoleForm key={user.id} tenant={tenant} user={user} />
          <IdentityList identities={identities} />
          <RemoveMemberSection
            tenant={tenant}
            user={user}
            users={users}
            self={self}
            onRemoved={() => onOpenChange(false)}
          />
        </div>
      </DialogContent>
    </Dialog>
  );
}

function RoleForm({ tenant, user }: { tenant: Tenant; user: User }) {
  const t = useTranslations("usersPage");
  const tTokens = useTranslations("tokens");
  const queryClient = useQueryClient();
  const [state, formAction, pending] = useActionState(
    updateTenantUserRole,
    null,
  );
  const [roleChanged, setRoleChanged] = useState(false);

  useMutationFeedback(state, {
    successMessage: t("roleUpdated"),
    onSuccess: () =>
      void invalidateTenantResources(
        queryClient,
        tenant.id,
        mutationResources.user,
      ),
  });

  return (
    <form action={formAction} className="grid gap-1.5">
      <input name="tenant_id" type="hidden" value={tenant.id} />
      <input name="user_id" type="hidden" value={user.id} />
      <span className="text-xs font-medium text-muted-foreground">
        {t("roleLabel")}
      </span>
      <div className="flex items-center gap-2">
        <select
          aria-label={t("roleLabel")}
          className={`${inputSmClasses} flex-1`}
          name="role"
          defaultValue={user.role}
          onChange={(event) => setRoleChanged(event.target.value !== user.role)}
        >
          {roles.map((candidate) => (
            <option key={candidate} value={candidate}>
              {tTokens.has(candidate) ? tTokens(candidate) : candidate}
            </option>
          ))}
        </select>
        <Button
          disabled={pending || !roleChanged}
          type="submit"
          variant="outline"
        >
          {pending ? t("saving") : t("saveRole")}
        </Button>
      </div>
    </form>
  );
}

function IdentityList({ identities }: { identities: UserIdentity[] }) {
  const t = useTranslations("usersPage");
  const formatDate = useAdminDate();

  return (
    <div className="grid gap-1.5">
      <span className="text-xs font-medium text-muted-foreground">
        {t("identitiesTitle")}
      </span>
      {identities.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("noIdentities")}</p>
      ) : (
        <ul className="grid gap-2">
          {identities.map((identity) => (
            <li
              key={identity.id}
              className="rounded-md border border-border px-3 py-2"
            >
              <div className="flex items-center justify-between gap-2">
                <Tag value={identity.provider} />
                <span className="text-xs text-muted-foreground">
                  {formatDate(identity.created_at)}
                </span>
              </div>
              <div className="mt-1 break-all font-mono text-xs text-muted-foreground">
                {identity.subject}
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function RemoveMemberSection({
  tenant,
  user,
  users,
  self,
  onRemoved,
}: {
  tenant: Tenant;
  user: User;
  users: User[];
  self: boolean;
  onRemoved: () => void;
}) {
  const t = useTranslations("usersPage");
  const queryClient = useQueryClient();
  const [state, formAction] = useActionState(removeTenantUser, null);

  useMutationFeedback(state, {
    successMessage: t("memberRemoved"),
    errorMessage: (code) =>
      t.has(`removeError.${code}`) ? t(`removeError.${code}`) : code,
    onSuccess: () => {
      onRemoved();
      void invalidateTenantResources(
        queryClient,
        tenant.id,
        mutationResources.user,
      );
    },
  });

  const lastAdmin = isLastTenantAdmin(user, users);
  const blocked = self || lastAdmin;

  return (
    <div className="rounded-md border border-destructive/40 px-3 py-3">
      <div className="text-sm font-medium text-foreground">
        {t("removeTitle")}
      </div>
      <p className="mt-0.5 text-xs text-muted-foreground">
        {t("removeHint")}
      </p>
      {blocked ? (
        <p className="mt-2 text-xs font-medium text-muted-foreground">
          {self ? t("removeSelfBlocked") : t("removeLastAdminBlocked")}
        </p>
      ) : (
        <ConfirmForm
          action={formAction}
          buttonAriaLabel={t("removeFor", { user: user.display_name })}
          buttonClassName="mt-2"
          buttonVariant="destructive"
          buttonLabel={
            <span className="inline-flex items-center gap-1">
              <TrashIcon aria-hidden="true" className="size-3.5" />
              {t("removeButton")}
            </span>
          }
          confirmLabel={t("removeConfirm")}
          message={t("removeMessage", { user: user.display_name })}
          title={t("removeTitle")}
          tone="danger"
        >
          <input name="tenant_id" type="hidden" value={tenant.id} />
          <input name="user_id" type="hidden" value={user.id} />
        </ConfirmForm>
      )}
    </div>
  );
}
