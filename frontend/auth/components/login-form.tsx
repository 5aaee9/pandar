"use client";

import { FormEvent, useEffect, useState } from "react";
import { GalleryVerticalEnd } from "lucide-react";

import { authClient } from "@/lib/auth-client";
import type { SignInMessages } from "@/lib/i18n";
import { redirectWithAuthToken } from "@/lib/token";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";

type LoginFormProps = React.ComponentProps<"div"> & {
  dashboardCallbackUrl: string;
  messages: SignInMessages;
};

const RESEND_COOLDOWN_SECONDS = 60;

function formatCooldown(template: string, seconds: number): string {
  return template.replace("{seconds}", String(seconds));
}

export function LoginForm({
  className,
  dashboardCallbackUrl,
  messages,
  ...props
}: LoginFormProps) {
  const [pending, setPending] = useState<"magic-link" | "passkey" | null>(null);
  const [sent, setSent] = useState(false);
  const [email, setEmail] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [cooldown, setCooldown] = useState(0);

  useEffect(() => {
    if (cooldown <= 0) {
      return;
    }

    const timer = window.setTimeout(() => {
      setCooldown((current) => Math.max(0, current - 1));
    }, 1000);

    return () => window.clearTimeout(timer);
  }, [cooldown]);

  async function sendMagicLink(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending || (sent && cooldown > 0)) {
      return;
    }

    const normalizedEmail = email.trim().toLowerCase();
    const name = normalizedEmail.split("@", 1)[0] || normalizedEmail;
    setPending("magic-link");
    setError(null);

    try {
      const result = await authClient.signIn.magicLink({
        email: normalizedEmail,
        name,
        callbackURL: "/auth/complete",
        newUserCallbackURL: "/auth/complete",
        errorCallbackURL: "/sign-in",
      });
      if (result.error) {
        throw new Error(result.error.message || messages.magicLinkSendFailed);
      }

      setSent(true);
      setCooldown(RESEND_COOLDOWN_SECONDS);
    } catch {
      setError(messages.unableSignIn);
    } finally {
      setPending(null);
    }
  }

  async function signInWithPasskey() {
    if (pending) {
      return;
    }

    setPending("passkey");
    setError(null);

    try {
      const result = await authClient.signIn.passkey();
      if (result.error) {
        throw new Error(result.error.message || messages.passkeySignInFailed);
      }

      await redirectWithAuthToken(dashboardCallbackUrl, messages);
    } catch {
      setError(messages.passkeySignInFailed);
      setPending(null);
    }
  }

  return (
    <div className={cn("flex flex-col gap-6", className)} {...props}>
      <form onSubmit={sendMagicLink}>
        <FieldGroup>
          <div className="flex flex-col items-center gap-2 text-center">
            <div className="flex flex-col items-center gap-2 font-medium">
              <div className="flex size-8 items-center justify-center rounded-md">
                <GalleryVerticalEnd className="size-6" />
              </div>
              <span className="sr-only">Pandar</span>
            </div>
            <h1 className="text-xl font-bold">{messages.signIn}</h1>
            <FieldDescription>{messages.signInIntro}</FieldDescription>
          </div>
          <Field data-invalid={error ? "true" : undefined}>
            <FieldLabel htmlFor="email">{messages.email}</FieldLabel>
            <Input
              id="email"
              autoComplete="email"
              inputMode="email"
              name="email"
              placeholder="name@example.com"
              required
              type="email"
              value={email}
              onChange={(event) => setEmail(event.currentTarget.value)}
            />
            {error ? (
              <FieldError>
                <span>{messages.magicLinkSendFailed}</span>
                <span>{error}</span>
              </FieldError>
            ) : null}
          </Field>
          {sent ? (
            <Field>
              <FieldDescription className="rounded-md border bg-muted/40 p-3 text-center">
                <span className="block font-medium text-foreground">
                  {messages.magicLinkEmailSent}
                </span>
                <span className="mt-1 block">{messages.magicLinkSentBody}</span>
                <span className="mt-1 block">
                  {messages.magicLinkCheckInbox}
                </span>
              </FieldDescription>
            </Field>
          ) : null}
          <Field>
            <Button
              disabled={pending !== null || (sent && cooldown > 0)}
              type="submit"
            >
              {pending === "magic-link"
                ? messages.magicLinkSending
                : sent && cooldown > 0
                  ? formatCooldown(messages.magicLinkResendCooldown, cooldown)
                  : sent
                    ? messages.magicLinkResend
                    : messages.magicLinkSubmit}
            </Button>
          </Field>
        </FieldGroup>
      </form>
      <div className="grid gap-4">
        <div className="flex items-center gap-3 text-xs text-muted-foreground">
          <span className="h-px flex-1 bg-border" />
          <span>{messages.or}</span>
          <span className="h-px flex-1 bg-border" />
        </div>
        <Button
          disabled={pending !== null}
          type="button"
          variant="outline"
          onClick={signInWithPasskey}
        >
          {pending === "passkey"
            ? messages.passkeySigningIn
            : messages.passkeySignIn}
        </Button>
      </div>
    </div>
  );
}
