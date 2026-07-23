"use client";

import { FormEvent, useCallback, useEffect, useReducer, useRef } from "react";
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
  completionUrl: string;
  dashboardCallbackUrl: string;
  errorUrl: string;
  messages: SignInMessages;
};

const RESEND_COOLDOWN_SECONDS = 60;

type LoginState = {
  pending: "magic-link" | "passkey" | null;
  sent: boolean;
  email: string;
  error: string | null;
  cooldown: number;
};

type LoginAction =
  | { type: "email"; value: string }
  | { type: "pending"; value: LoginState["pending"] }
  | { type: "error"; value: string | null }
  | { type: "sent"; value: boolean }
  | { type: "cooldown"; value: number }
  | { type: "tick" };

const initialLoginState: LoginState = {
  pending: null,
  sent: false,
  email: "",
  error: null,
  cooldown: 0,
};

function loginReducer(state: LoginState, action: LoginAction): LoginState {
  switch (action.type) {
    case "email":
      return { ...state, email: action.value };
    case "pending":
      return { ...state, pending: action.value };
    case "error":
      return { ...state, error: action.value };
    case "sent":
      return { ...state, sent: action.value };
    case "cooldown":
      return { ...state, cooldown: action.value };
    case "tick":
      return { ...state, cooldown: Math.max(0, state.cooldown - 1) };
  }
}

function formatCooldown(template: string, seconds: number): string {
  return template.replace("{seconds}", String(seconds));
}

export function LoginForm({
  className,
  completionUrl,
  dashboardCallbackUrl,
  errorUrl,
  messages,
  ...props
}: LoginFormProps) {
  const [{ pending, sent, email, error, cooldown }, dispatchLogin] =
    useReducer(loginReducer, initialLoginState);
  const redirectStartedRef = useRef(false);

  const redirectAfterPasskeySignIn = useCallback(async () => {
    if (redirectStartedRef.current) {
      return;
    }

    redirectStartedRef.current = true;
    try {
      await redirectWithAuthToken(dashboardCallbackUrl, messages);
    } catch (error) {
      redirectStartedRef.current = false;
      throw error;
    }
  }, [dashboardCallbackUrl, messages]);

  useEffect(() => {
    if (cooldown <= 0) {
      return;
    }

    const timer = window.setTimeout(() => {
      dispatchLogin({ type: "tick" });
    }, 1000);

    return () => window.clearTimeout(timer);
  }, [cooldown]);

  useEffect(() => {
    let active = true;

    async function preloadPasskeyAutofill() {
      const publicKeyCredential = window.PublicKeyCredential;
      if (!publicKeyCredential?.isConditionalMediationAvailable) {
        return;
      }

      try {
        const available =
          await publicKeyCredential.isConditionalMediationAvailable();
        if (!available || !active) {
          return;
        }

        const result = await authClient.signIn.passkey({ autoFill: true });
        if (!active) {
          return;
        }
        if (result.error) {
          throw result.error;
        }

        await redirectAfterPasskeySignIn();
      } catch (error) {
        if (active) {
          console.warn("Passkey autofill sign-in failed", error);
        }
      }
    }

    void preloadPasskeyAutofill();

    return () => {
      active = false;
    };
  }, [redirectAfterPasskeySignIn]);

  async function sendMagicLink(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending || (sent && cooldown > 0)) {
      return;
    }

    const normalizedEmail = email.trim().toLowerCase();
    const name = normalizedEmail.split("@", 1)[0] || normalizedEmail;
    dispatchLogin({ type: "pending", value: "magic-link" });
    dispatchLogin({ type: "error", value: null });

    try {
      const result = await authClient.signIn.magicLink({
        email: normalizedEmail,
        name,
        callbackURL: completionUrl,
        newUserCallbackURL: completionUrl,
        errorCallbackURL: errorUrl,
      });
      if (result.error) {
        throw new Error(result.error.message || messages.magicLinkSendFailed);
      }

      dispatchLogin({ type: "sent", value: true });
      dispatchLogin({ type: "cooldown", value: RESEND_COOLDOWN_SECONDS });
    } catch {
      dispatchLogin({ type: "error", value: messages.unableSignIn });
    } finally {
      dispatchLogin({ type: "pending", value: null });
    }
  }

  async function signInWithPasskey() {
    if (pending) {
      return;
    }

    dispatchLogin({ type: "pending", value: "passkey" });
    dispatchLogin({ type: "error", value: null });

    try {
      const result = await authClient.signIn.passkey();
      if (result.error) {
        throw new Error(result.error.message || messages.passkeySignInFailed);
      }

      await redirectAfterPasskeySignIn();
    } catch {
      dispatchLogin({ type: "error", value: messages.passkeySignInFailed });
      dispatchLogin({ type: "pending", value: null });
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
              autoComplete="username webauthn"
              inputMode="email"
              name="email"
              placeholder="name@example.com"
              required
              type="email"
              value={email}
              onChange={(event) =>
                dispatchLogin({
                  type: "email",
                  value: event.currentTarget.value,
                })
              }
            />
            {error ? (
              <FieldError className="auth-feedback-enter">
                <span>{messages.magicLinkSendFailed}</span>
                <span>{error}</span>
              </FieldError>
            ) : null}
          </Field>
          {sent ? (
            <Field>
              <FieldDescription className="auth-feedback-enter rounded-md border bg-muted/40 p-3 text-center">
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
