import { env } from "../../../lib/env";
import { getAuthLocale, getAuthMessages } from "../../../lib/i18n";
import {
  normalizePluginReturnTo,
  withPluginReturnTo,
} from "../../../lib/plugin-return";
import { CompleteAuth } from "./complete-auth";

export const dynamic = "force-dynamic";

type PageProps = {
  searchParams?: Promise<{ return_to?: string | string[] }>;
};

export default async function CompleteAuthPage({ searchParams }: PageProps) {
  const messages = getAuthMessages(await getAuthLocale());
  const returnTo = normalizePluginReturnTo((await searchParams)?.return_to);

  return (
    <main className="auth-page">
      <section className="auth-panel" aria-labelledby="complete-auth-title">
        <h1 id="complete-auth-title">{messages.addPasskey}</h1>
        <p>{messages.passkeyOptionalIntro}</p>
        <CompleteAuth
          dashboardCallbackUrl={withPluginReturnTo(
            env.dashboardCallbackUrl,
            returnTo,
          )}
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
