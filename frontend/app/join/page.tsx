import { getTranslations } from "next-intl/server";

import { acceptJoinLink } from "../actions";
import { authProviderConfig } from "../auth-provider";
import { SectionHeader } from "../dashboard-ui";
import { JoinTokenForm } from "./token-form";

export default async function JoinPage() {
  const t = await getTranslations("join");
  const auth = authProviderConfig();

  return (
    <main className="min-h-screen bg-slate-100 px-4 py-5 text-slate-950 sm:px-6 lg:px-8">
      <section className="mx-auto max-w-xl overflow-hidden rounded-md border border-slate-300 bg-white">
        <SectionHeader
          title={t("title")}
          subtitle={t("subtitle", { provider: auth.provider })}
          meta={auth.cookieName}
        />
        <ProviderLinks
          signInUrl={auth.signInUrl}
          signOutUrl={auth.signOutUrl}
        />
        <JoinTokenForm action={acceptJoinLink} />
      </section>
    </main>
  );
}

async function ProviderLinks({
  signInUrl,
  signOutUrl,
}: {
  signInUrl: string | null;
  signOutUrl: string | null;
}) {
  const t = await getTranslations("join");
  if (!signInUrl && !signOutUrl) {
    return null;
  }

  return (
    <div className="flex flex-wrap gap-2 border-b border-slate-200 px-4 py-3">
      {signInUrl ? (
        <a
          className="inline-flex h-8 items-center rounded-md border border-slate-300 px-3 text-sm font-medium"
          href={signInUrl}
        >
          {t("signIn")}
        </a>
      ) : null}
      {signOutUrl ? (
        <a
          className="inline-flex h-8 items-center rounded-md border border-slate-300 px-3 text-sm font-medium"
          href={signOutUrl}
        >
          {t("signOut")}
        </a>
      ) : null}
    </div>
  );
}
