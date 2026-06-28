import { env } from "../../../lib/env";
import { getAuthLocale, getAuthMessages } from "../../../lib/i18n";
import { CompleteAuth } from "./complete-auth";

export const dynamic = "force-dynamic";

export default async function CompleteAuthPage() {
  const messages = getAuthMessages(await getAuthLocale());

  return (
    <main className="auth-page">
      <section className="auth-panel" aria-labelledby="complete-auth-title">
        <h1 id="complete-auth-title">{messages.addPasskey}</h1>
        <p>{messages.passkeyOptionalIntro}</p>
        <CompleteAuth
          dashboardCallbackUrl={env.dashboardCallbackUrl}
          messages={{
            addPasskey: messages.addPasskey,
            addingPasskey: messages.addingPasskey,
            continueDashboard: messages.continueDashboard,
            dashboardTokenEmpty: messages.dashboardTokenEmpty,
            dashboardTokenFailed: messages.dashboardTokenFailed,
            passkeyAdded: messages.passkeyAdded,
            passkeyAddFailed: messages.passkeyAddFailed,
            passkeyOptionalIntro: messages.passkeyOptionalIntro,
            returningDashboard: messages.returningDashboard,
            skipPasskey: messages.skipPasskey,
          }}
        />
      </section>
    </main>
  );
}
