import { LoginForm } from "../../components/login-form";
import { env } from "../../lib/env";
import { getAuthLocale, getAuthMessages } from "../../lib/i18n";
import {
  normalizeDashboardState,
  withDashboardState,
} from "../../lib/dashboard-state";
import {
  normalizePluginReturnTo,
  withPluginReturnTo,
} from "../../lib/plugin-return";

export const dynamic = "force-dynamic";

type PageProps = {
  searchParams?: Promise<{
    return_to?: string | string[];
    state?: string | string[];
  }>;
};

export default async function SignInPage({ searchParams }: PageProps) {
  const messages = getAuthMessages(await getAuthLocale());
  const params = await searchParams;
  const returnTo = normalizePluginReturnTo(params?.return_to);
  const state = normalizeDashboardState(params?.state);

  return (
    <main className="flex min-h-svh flex-col items-center justify-center gap-6 bg-background p-6 md:p-10">
      <div className="w-full max-w-sm">
        <LoginForm
          completionUrl={withDashboardState(
            withPluginReturnTo("/auth/complete", returnTo),
            state,
          )}
          dashboardCallbackUrl={withDashboardState(
            withPluginReturnTo(env.dashboardCallbackUrl, returnTo),
            state,
          )}
          errorUrl={withDashboardState(
            withPluginReturnTo("/sign-in", returnTo),
            state,
          )}
          messages={{
            dashboardTokenEmpty: messages.dashboardTokenEmpty,
            dashboardTokenFailed: messages.dashboardTokenFailed,
            email: messages.email,
            magicLinkCheckInbox: messages.magicLinkCheckInbox,
            magicLinkEmailSent: messages.magicLinkEmailSent,
            magicLinkResend: messages.magicLinkResend,
            magicLinkResendCooldown: messages.magicLinkResendCooldown,
            magicLinkSendFailed: messages.magicLinkSendFailed,
            magicLinkSubmit: messages.magicLinkSubmit,
            magicLinkSentBody: messages.magicLinkSentBody,
            magicLinkSending: messages.magicLinkSending,
            or: messages.or,
            passkeySignIn: messages.passkeySignIn,
            passkeySignInFailed: messages.passkeySignInFailed,
            passkeySigningIn: messages.passkeySigningIn,
            signIn: messages.signIn,
            signInIntro: messages.signInIntro,
            unableSignIn: messages.unableSignIn,
          }}
        />
      </div>
    </main>
  );
}
