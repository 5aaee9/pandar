import { env } from "../../lib/env";
import { getAuthLocale, getAuthMessages } from "../../lib/i18n";
import { SignOutClient } from "./sign-out-client";

export const dynamic = "force-dynamic";

export default async function SignOutPage() {
  const messages = getAuthMessages(await getAuthLocale());

  return (
    <main className="auth-page">
      <section className="auth-panel" aria-labelledby="sign-out-title">
        <h1 id="sign-out-title">{messages.signingOut}</h1>
        <p>{messages.signingOutIntro}</p>
        <SignOutClient
          dashboardSignOutUrl={env.dashboardSignOutUrl}
          messages={{
            clearingIssuerSession: messages.clearingIssuerSession,
            returningDashboard: messages.returningDashboard,
            returnToDashboard: messages.returnToDashboard,
            retrySignOut: messages.retrySignOut,
            signedOut: messages.signedOut,
            signingOutIntro: messages.signingOutIntro,
            signOutWarning: messages.signOutWarning,
            unableSignOut: messages.unableSignOut,
          }}
        />
      </section>
    </main>
  );
}
