import { env } from "../../lib/env";
import { SignOutClient } from "./sign-out-client";

export const dynamic = "force-dynamic";

export default function SignOutPage() {
  return (
    <main className="auth-page">
      <section className="auth-panel" aria-labelledby="sign-out-title">
        <h1 id="sign-out-title">Signing out</h1>
        <p>Clearing your issuer session before returning to Pandar.</p>
        <SignOutClient dashboardSignOutUrl={env.dashboardSignOutUrl} />
      </section>
    </main>
  );
}
