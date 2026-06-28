import { AuthSessionContext } from "../auth-session-context";
import { env } from "../../lib/env";
import { getAuthLocale, getAuthMessages } from "../../lib/i18n";
import { SignInForm } from "./sign-in-form";

export const dynamic = "force-dynamic";

export default async function SignInPage() {
  const messages = getAuthMessages(await getAuthLocale());

  return (
    <main className="auth-page">
      <section className="auth-shell" aria-labelledby="sign-in-title">
        <div className="auth-hero" aria-hidden="true" />
        <div className="auth-panel">
          <h1 id="sign-in-title">{messages.signIn}</h1>
          <p>{messages.signInIntro}</p>
          <AuthSessionContext
            dashboardCallbackUrl={env.dashboardCallbackUrl}
            issuerUrl={env.baseURL}
            jwtMaxAgeSeconds={env.jwtMaxAgeSeconds}
            messages={messages}
          />
          <SignInForm
            messages={{
              email: messages.email,
              magicLinkCheckInbox: messages.magicLinkCheckInbox,
              magicLinkEmailSent: messages.magicLinkEmailSent,
              magicLinkResend: messages.magicLinkResend,
              magicLinkResendCooldown: messages.magicLinkResendCooldown,
              magicLinkSendFailed: messages.magicLinkSendFailed,
              magicLinkSubmit: messages.magicLinkSubmit,
              magicLinkSentBody: messages.magicLinkSentBody,
              magicLinkSending: messages.magicLinkSending,
              unableSignIn: messages.unableSignIn,
            }}
          />
        </div>
      </section>
    </main>
  );
}
