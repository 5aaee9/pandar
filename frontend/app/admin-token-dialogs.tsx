import { useActionState, useId, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { PlusIcon } from "lucide-react";
import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  createTenantToken,
  revokeTenantToken,
  rotateTenantToken,
} from "./admin-actions";
import { Input, PrimaryButton, SecretActionResult } from "./admin-panel-shared";
import type { TenantToken } from "./dashboard-types";
import {
  mutationResources,
  useInvalidateOnSuccess,
} from "./mutation-invalidation";
import { useActionStatusFeedback } from "./mutation-feedback";

type TokenStatus = "active" | "expired" | "revoked";
export function CreateTenantTokenDialog({ tenantId }: { tenantId: string }) {
  const [sessionKey, setSessionKey] = useState(0);

  return (
    <CreateTenantTokenDialogSession
      key={sessionKey}
      tenantId={tenantId}
      onClosed={() => setSessionKey((current) => current + 1)}
    />
  );
}

function CreateTenantTokenDialogSession({
  tenantId,
  onClosed,
}: {
  tenantId: string;
  onClosed: () => void;
}) {
  const t = useTranslations("admin");
  const helpId = useId();
  const [open, setOpen] = useState(false);
  const queryClient = useQueryClient();
  const [state, formAction, pending] = useActionState(createTenantToken, null);
  useInvalidateOnSuccess(
    state,
    queryClient,
    tenantId,
    mutationResources.token,
  );
  const completed = state?.ok && state.kind === "tenant_token";

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (nextOpen) {
          setOpen(true);
        } else if (!pending) {
          onClosed();
        }
      }}
    >
      <DialogTrigger render={<Button />}>
        <PlusIcon />
        {t("createToken")}
      </DialogTrigger>
      <DialogContent
        className="sm:max-w-lg"
        closeLabel={t("close")}
        showCloseButton={!pending}
      >
        <DialogHeader>
          <DialogTitle>{t("createTenantToken")}</DialogTitle>
          <DialogDescription>{t("createTokenDescription")}</DialogDescription>
        </DialogHeader>
        <form
          action={formAction}
          className="grid gap-4"
          onSubmit={(event) => {
            if (completed) {
              event.preventDefault();
            }
          }}
        >
          <input name="tenant_id" type="hidden" value={tenantId} />
          <div className="grid gap-3 sm:grid-cols-2">
            <Input disabled={completed} name="name" label={t("name")} required />
            <Input
              defaultValue="*"
              describedBy={`${helpId}-scopes`}
              disabled={completed}
              name="scopes"
              label={t("scopes")}
            />
          </div>
          <p className="-mt-2 text-xs text-muted-foreground" id={`${helpId}-scopes`}>
            {t("scopesHelp")}
          </p>
          <Input
            name="expires_at"
            label={t("expiresAt")}
            describedBy={`${helpId}-expires`}
            disabled={completed}
            placeholder="2026-12-31T00:00:00Z"
          />
          <p className="-mt-2 text-xs text-muted-foreground" id={`${helpId}-expires`}>
            {t("expiresAtHelp")}
          </p>
          <SecretActionResult state={state} />
          <DialogFooter>
            {completed ? (
              <DialogClose render={<Button type="button" variant="outline" />}>
                {t("close")}
              </DialogClose>
            ) : (
              <>
                <DialogClose
                  render={
                    <Button
                      disabled={pending}
                      type="button"
                      variant="outline"
                    />
                  }
                >
                  {t("cancel")}
                </DialogClose>
                <PrimaryButton
                  disabled={pending}
                  label={pending ? t("creating") : t("createToken")}
                />
              </>
            )}
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function RotateTenantTokenForm({
  tenantId,
  token,
  status,
}: {
  tenantId: string;
  token: TenantToken;
  status: TokenStatus;
}) {
  const [sessionKey, setSessionKey] = useState(0);

  return (
    <RotateTenantTokenDialogSession
      key={sessionKey}
      tenantId={tenantId}
      token={token}
      status={status}
      onClosed={() => setSessionKey((current) => current + 1)}
    />
  );
}

function RotateTenantTokenDialogSession({
  tenantId,
  token,
  status,
  onClosed,
}: {
  tenantId: string;
  token: TenantToken;
  status: TokenStatus;
  onClosed: () => void;
}) {
  const t = useTranslations("admin");
  const [open, setOpen] = useState(false);
  const queryClient = useQueryClient();
  const [state, formAction, pending] = useActionState(rotateTenantToken, null);
  useInvalidateOnSuccess(
    state,
    queryClient,
    tenantId,
    mutationResources.token,
  );
  const completed = state?.ok && state.kind === "tenant_token";

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (nextOpen) {
          setOpen(true);
        } else if (!pending) {
          onClosed();
        }
      }}
    >
      {status !== "revoked" ? (
        <DialogTrigger
          render={
            <Button
              aria-label={t("rotateFor", { name: token.name })}
              className="flex-1 md:flex-none"
              size="sm"
              variant="outline"
            />
          }
        >
          {t("rotate")}
        </DialogTrigger>
      ) : null}
      <DialogContent
        className="sm:max-w-lg"
        closeLabel={t("close")}
        showCloseButton={!pending}
      >
        <DialogHeader>
          <DialogTitle>{t("rotateTokenTitle")}</DialogTitle>
          <DialogDescription>
            {t("rotateTokenMessage", { name: token.name })}
          </DialogDescription>
        </DialogHeader>
        <form
          action={formAction}
          className="grid gap-4"
          onSubmit={(event) => {
            if (completed) {
              event.preventDefault();
            }
          }}
        >
          <input name="tenant_id" type="hidden" value={tenantId} />
          <input name="token_id" type="hidden" value={token.id} />
          <Input
            name="expires_at"
            label={t("expiresAt")}
            disabled={completed}
            defaultValue={status === "expired" ? "" : (token.expires_at ?? "")}
            placeholder="2026-12-31T00:00:00Z"
          />
          <p className="-mt-2 text-xs text-muted-foreground">
            {t(status === "expired" ? "rotateExpiredHelp" : "rotateExpiryHelp")}
          </p>
          <SecretActionResult state={state} />
          <DialogFooter>
            {completed ? (
              <DialogClose render={<Button type="button" variant="outline" />}>
                {t("close")}
              </DialogClose>
            ) : (
              <>
                <DialogClose
                  render={
                    <Button
                      disabled={pending}
                      type="button"
                      variant="outline"
                    />
                  }
                >
                  {t("cancel")}
                </DialogClose>
                <PrimaryButton
                  disabled={pending}
                  label={pending ? t("rotating") : t("rotateTokenConfirm")}
                />
              </>
            )}
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function RevokeTenantTokenDialog({
  tenantId,
  token,
}: {
  tenantId: string;
  token: TenantToken;
}) {
  const t = useTranslations("admin");
  const [open, setOpen] = useState(false);
  const revokeAction = useActionStatusFeedback(
    revokeTenantToken,
    "tenant_token_revoked",
    {
      invalidate: mutationResources.token,
      onSuccess: () => setOpen(false),
    },
  );

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (nextOpen || !revokeAction.pending) {
          setOpen(nextOpen);
        }
      }}
    >
      <DialogTrigger
        render={
          <Button
            aria-label={t("revokeFor", { name: token.name })}
            className="flex-1 md:flex-none"
            size="sm"
            variant="destructive"
          />
        }
      >
        {t("revoke")}
      </DialogTrigger>
      <DialogContent
        className="sm:max-w-lg"
        closeLabel={t("close")}
        showCloseButton={!revokeAction.pending}
      >
        <DialogHeader>
          <DialogTitle>{t("revokeTokenTitle")}</DialogTitle>
          <DialogDescription>
            {t("revokeTokenMessage", { name: token.name })}
          </DialogDescription>
        </DialogHeader>
        <form action={revokeAction.formAction} className="grid gap-4">
          <input name="tenant_id" type="hidden" value={tenantId} />
          <input name="token_id" type="hidden" value={token.id} />
          <DialogFooter>
            <DialogClose
              render={
                <Button disabled={revokeAction.pending} type="button" variant="outline" />
              }
            >
              {t("cancel")}
            </DialogClose>
            <Button disabled={revokeAction.pending} type="submit" variant="destructive">
              {revokeAction.pending ? t("revoking") : t("revokeTokenConfirm")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
