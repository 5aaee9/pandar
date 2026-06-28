import Link from "next/link";

import { AuthSessionContext } from "../auth-session-context";
import { env } from "../../lib/env";
import { getAuthLocale, getAuthMessages } from "../../lib/i18n";
import { SignUpForm } from "./sign-up-form";

export const dynamic = "force-dynamic";

export default async function SignUpPage() {
  const messages = getAuthMessages(await getAuthLocale());

  return (
    <main className="auth-page">
      <section className="auth-panel" aria-labelledby="sign-up-title">
        <h1 id="sign-up-title">{messages.createAccount}</h1>
        <p>{messages.createAccountIntro}</p>
        <AuthSessionContext
          dashboardCallbackUrl={env.dashboardCallbackUrl}
          issuerUrl={env.baseURL}
          jwtMaxAgeSeconds={env.jwtMaxAgeSeconds}
          messages={messages}
        />
        <SignUpForm
          dashboardCallbackUrl={env.dashboardCallbackUrl}
          messages={{
            createAccountWithPasskey: messages.createAccountWithPasskey,
            dashboardTokenEmpty: messages.dashboardTokenEmpty,
            dashboardTokenFailed: messages.dashboardTokenFailed,
            deviceConfirmation: messages.deviceConfirmation,
            email: messages.email,
            name: messages.name,
            passkeyRegistrationFailed: messages.passkeyRegistrationFailed,
            passkeySignInFailed: messages.passkeySignInFailed,
            registerFailed: messages.registerFailed,
            signingUp: messages.signingUp,
            unableCreateAccount: messages.unableCreateAccount,
          }}
        />
        <div className="auth-actions">
          <Link className="auth-link" href="/sign-in">
            {messages.alreadyHaveAccount}
          </Link>
        </div>
      </section>
    </main>
  );
}
