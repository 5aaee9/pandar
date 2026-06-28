import Link from "next/link";

import { AuthSessionContext } from "../auth-session-context";
import { env } from "../../lib/env";
import { getAuthLocale, getAuthMessages } from "../../lib/i18n";
import { SignInForm } from "./sign-in-form";

export const dynamic = "force-dynamic";

export default async function SignInPage() {
  const messages = getAuthMessages(await getAuthLocale());

  return (
    <main className="auth-page">
      <section className="auth-panel" aria-labelledby="sign-in-title">
        <h1 id="sign-in-title">{messages.signIn}</h1>
        <p>{messages.signInIntro}</p>
        <AuthSessionContext
          dashboardCallbackUrl={env.dashboardCallbackUrl}
          issuerUrl={env.baseURL}
          jwtMaxAgeSeconds={env.jwtMaxAgeSeconds}
          messages={messages}
        />
        <SignInForm
          dashboardCallbackUrl={env.dashboardCallbackUrl}
          messages={{
            dashboardTokenEmpty: messages.dashboardTokenEmpty,
            dashboardTokenFailed: messages.dashboardTokenFailed,
            passkeySignInFailed: messages.passkeySignInFailed,
            signInFailed: messages.signInFailed,
            signingIn: messages.signingIn,
            signInWithPasskey: messages.signInWithPasskey,
            unableSignIn: messages.unableSignIn,
          }}
        />
        <div className="auth-actions">
          <Link className="auth-link" href="/sign-up">
            {messages.createAccount}
          </Link>
        </div>
      </section>
    </main>
  );
}
