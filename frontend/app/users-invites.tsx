"use client";

import { useActionState, useMemo, useState } from "react";
import { useLocale, useTranslations } from "next-intl";
import { LinkIcon, PlusIcon } from "lucide-react";

import { createJoinLink, revokeJoinLink } from "./admin-actions";
import { roles, useAdminDate } from "./admin-model";
import {
  Input,
  PrimaryButton,
  SecretActionResult,
} from "./admin-panel-shared";
import { ConfirmForm } from "./confirm-dialog";
import type { JoinLink, Tenant } from "./dashboard-types";
import { EmptyState, SectionHeader, Tag } from "./dashboard-ui";
import { getRelativeTime } from "./dayjs-relative-time";
import { rowHoverClasses } from "../lib/utils";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DEFAULT_INVITE_TTL,
  INVITE_TTL_OPTIONS,
  inviteStatus,
  sortJoinLinks,
} from "./users-model";
import { useMutationFeedback } from "./mutation-feedback";
import {
  invalidateTenantResources,
  mutationResources,
} from "./mutation-invalidation";
import { useQueryClient } from "@tanstack/react-query";
import { InviteStatusChip, useNowMs } from "./users-shared";

export function InvitesSection({
  tenant,
  joinLinks,
}: {
  tenant: Tenant;
  joinLinks: JoinLink[];
}) {
  const t = useTranslations("usersPage");
  const nowMs = useNowMs();
  const [createOpen, setCreateOpen] = useState(false);
  const sorted = useMemo(
    () => sortJoinLinks(joinLinks, nowMs),
    [joinLinks, nowMs],
  );
  const activeCount = useMemo(
    () => sorted.filter((link) => inviteStatus(link, nowMs) === "active").length,
    [sorted, nowMs],
  );

  return (
    <section className="overflow-hidden rounded-md border border-border bg-card">
      <SectionHeader
        title={t("invitesTitle")}
        subtitle={t("invitesSubtitle", { name: tenant.display_name })}
        meta={t("invitesMeta", { active: activeCount, total: joinLinks.length })}
        actions={
          <Button
            className="h-8 gap-1.5 px-2.5"
            onClick={() => setCreateOpen(true)}
            size="default"
            type="button"
          >
            <PlusIcon aria-hidden="true" className="size-4" />
            {t("createInvite")}
          </Button>
        }
      />
      {joinLinks.length === 0 ? (
        <EmptyState title={t("noInvitesTitle")} message={t("noInvitesMessage")} />
      ) : (
        <div className="divide-y divide-border">
          {sorted.map((link) => (
            <InviteRow
              key={link.id}
              link={link}
              nowMs={nowMs}
              tenant={tenant}
            />
          ))}
        </div>
      )}
      <CreateInviteDialog
        onOpenChange={setCreateOpen}
        open={createOpen}
        tenant={tenant}
      />
    </section>
  );
}

function InviteRow({
  tenant,
  link,
  nowMs,
}: {
  tenant: Tenant;
  link: JoinLink;
  nowMs: number;
}) {
  const t = useTranslations("usersPage");
  const locale = useLocale();
  const formatDate = useAdminDate();
  const status = inviteStatus(link, nowMs);
  const relative = getRelativeTime(link.expires_at, nowMs, locale);
  const usagePercent = Math.min(
    100,
    Math.round((link.used_count / link.max_uses) * 100),
  );

  return (
    <div
      className={`grid gap-3 px-4 py-3 text-sm ${rowHoverClasses} lg:grid-cols-[minmax(0,1fr)_auto] lg:items-center`}
    >
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <InviteStatusChip status={status} />
          <Tag value={link.role} />
          <span className="text-xs text-muted-foreground">
            {t("usage", { used: link.used_count, max: link.max_uses })}
          </span>
        </div>
        <div className="mt-1.5 text-xs text-muted-foreground">
          {link.email_constraint
            ? t("emailConstraint", { email: link.email_constraint })
            : t("anyEmail")}
          {" · "}
          <span title={formatDate(link.expires_at)}>
            {relative
              ? status === "expired"
                ? t("expiredRelative", { relative: relative.relative })
                : t("expiresRelative", { relative: relative.relative })
              : formatDate(link.expires_at)}
          </span>
        </div>
        <div className="mt-2 h-1 w-32 overflow-hidden rounded-full bg-muted">
          <div
            className={`h-full rounded-full ${status === "active" ? "bg-primary" : "bg-muted-foreground/40"}`}
            style={{ width: `${usagePercent}%` }}
          />
        </div>
      </div>
      {status === "active" ? (
        <RevokeInviteButton link={link} tenant={tenant} />
      ) : null}
    </div>
  );
}

function RevokeInviteButton({
  tenant,
  link,
}: {
  tenant: Tenant;
  link: JoinLink;
}) {
  const t = useTranslations("usersPage");
  const queryClient = useQueryClient();
  const [state, formAction] = useActionState(revokeJoinLink, null);

  useMutationFeedback(state, {
    successMessage: t("inviteRevoked"),
    onSuccess: () =>
      void invalidateTenantResources(
        queryClient,
        tenant.id,
        mutationResources.joinLink,
      ),
  });

  return (
    <ConfirmForm
      action={formAction}
      buttonAriaLabel={t("revokeFor", { id: link.id })}
      buttonLabel={t("revoke")}
      buttonVariant="destructive"
      confirmLabel={t("revokeConfirm")}
      message={t("revokeMessage")}
      title={t("revokeTitle")}
      tone="danger"
    >
      <input name="tenant_id" type="hidden" value={tenant.id} />
      <input name="join_link_id" type="hidden" value={link.id} />
    </ConfirmForm>
  );
}

function CreateInviteDialog({
  tenant,
  open,
  onOpenChange,
}: {
  tenant: Tenant;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const t = useTranslations("usersPage");
  const [nonce, setNonce] = useState(0);

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        onOpenChange(nextOpen);
        if (!nextOpen) {
          setNonce((value) => value + 1);
        }
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("createInviteTitle")}</DialogTitle>
          <DialogDescription>
            {t("createInviteDescription", { name: tenant.display_name })}
          </DialogDescription>
        </DialogHeader>
        <CreateInviteForm
          key={nonce}
          onCreateAnother={() => setNonce((value) => value + 1)}
          tenant={tenant}
        />
      </DialogContent>
    </Dialog>
  );
}

function CreateInviteForm({
  tenant,
  onCreateAnother,
}: {
  tenant: Tenant;
  onCreateAnother: () => void;
}) {
  const t = useTranslations("usersPage");
  const tTokens = useTranslations("tokens");
  const queryClient = useQueryClient();
  const [state, formAction, pending] = useActionState(createJoinLink, null);
  const [selectedRole, setSelectedRole] =
    useState<(typeof roles)[number]>("viewer");
  const locked = pending || state?.ok === true;

  useMutationFeedback(state, {
    silentError: true,
    onSuccess: () =>
      void invalidateTenantResources(
        queryClient,
        tenant.id,
        mutationResources.joinLink,
      ),
  });

  return (
    <form action={formAction} className="grid gap-4">
      <input name="tenant_id" type="hidden" value={tenant.id} />
      <fieldset className="grid gap-4" disabled={locked}>
        <div className="grid gap-1.5">
          <span className="text-xs font-medium text-muted-foreground">
            {t("roleLabel")}
          </span>
          <div className="grid gap-2">
            {roles.map((role) => (
              <label
                className="flex cursor-pointer items-start gap-2.5 rounded-md border border-border px-3 py-2 transition-colors duration-150 ease-out has-checked:border-primary/50 has-checked:bg-primary/5"
                key={role}
              >
                <input
                  className="mt-0.5"
                  defaultChecked={selectedRole === role}
                  name="role"
                  onChange={() => setSelectedRole(role)}
                  type="radio"
                  value={role}
                />
                <span className="min-w-0">
                  <span className="block text-sm font-medium text-foreground">
                    {tTokens.has(role) ? tTokens(role) : role}
                  </span>
                  <span className="block text-xs text-muted-foreground">
                    {t(`roleHint.${role}`)}
                  </span>
                </span>
              </label>
            ))}
          </div>
        </div>
        <Input
          label={t("emailLabel")}
          name="email_constraint"
          placeholder={t("emailPlaceholder")}
          type="email"
        />
        <div className="grid gap-1.5">
          <span className="text-xs font-medium text-muted-foreground">
            {t("expiryLabel")}
          </span>
          <div className="flex flex-wrap gap-2">
            {INVITE_TTL_OPTIONS.map((option) => (
              <label className="cursor-pointer" key={option.id}>
                <input
                  className="peer sr-only"
                  defaultChecked={option.id === DEFAULT_INVITE_TTL}
                  name="expires_in_seconds"
                  type="radio"
                  value={option.seconds}
                />
                <span className="inline-flex h-8 items-center rounded-md border border-border px-3 text-xs font-medium text-muted-foreground transition-colors duration-150 ease-out peer-checked:border-primary/50 peer-checked:bg-primary/10 peer-checked:text-primary">
                  {t(`expiry.${option.id}`)}
                </span>
              </label>
            ))}
          </div>
        </div>
        <Input
          defaultValue="1"
          label={t("maxUsesLabel")}
          min="1"
          name="max_uses"
          required
          type="number"
        />
      </fieldset>
      {locked ? null : (
        <PrimaryButton label={pending ? t("creating") : t("create")} />
      )}
      <SecretActionResult state={state} />
      {state?.ok ? (
        <Button
          className="h-auto gap-1 self-start px-0 text-xs"
          onClick={onCreateAnother}
          type="button"
          variant="link"
        >
          <LinkIcon aria-hidden="true" className="size-3" />
          {t("createAnother")}
        </Button>
      ) : null}
    </form>
  );
}
