import { useActionState, useState } from "react";
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
  const [open, setOpen] = useState(false);
  const [state, formAction, pending] = useActionState(createTenantToken, null);
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
            <Input disabled={completed} name="name" label={t("name")} />
            <Input
              defaultValue="*"
              disabled={completed}
              name="scopes"
              label={t("scopes")}
            />
          </div>
          <p className="-mt-2 text-xs text-muted-foreground">
            {t("scopesHelp")}
          </p>
          <Input
            name="expires_at"
            label={t("expiresAt")}
            disabled={completed}
            placeholder="2026-12-31T00:00:00Z"
          />
          <p className="-mt-2 text-xs text-muted-foreground">
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
  const [state, formAction, pending] = useActionState(rotateTenantToken, null);
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
          <DialogDescription>{t("rotateTokenMessage")}</DialogDescription>
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
  const [submitting, setSubmitting] = useState(false);

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (nextOpen || !submitting) {
          setOpen(nextOpen);
        }
      }}
    >
      <DialogTrigger
        render={
          <Button
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
        showCloseButton={!submitting}
      >
        <DialogHeader>
          <DialogTitle>{t("revokeTokenTitle")}</DialogTitle>
          <DialogDescription>{t("revokeTokenMessage")}</DialogDescription>
        </DialogHeader>
        <form
          action={revokeTenantToken}
          className="grid gap-4"
          onSubmit={() => setSubmitting(true)}
        >
          <input name="tenant_id" type="hidden" value={tenantId} />
          <input name="token_id" type="hidden" value={token.id} />
          <input name="return_to" type="hidden" value="settings" />
          <DialogFooter>
            <DialogClose
              render={
                <Button disabled={submitting} type="button" variant="outline" />
              }
            >
              {t("cancel")}
            </DialogClose>
            <Button disabled={submitting} type="submit" variant="destructive">
              {submitting ? t("revoking") : t("revokeTokenConfirm")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
