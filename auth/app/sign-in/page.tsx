import Link from "next/link";

import { env } from "../../lib/env";
import { SignInForm } from "./sign-in-form";

export const dynamic = "force-dynamic";

export default function SignInPage() {
  return (
    <main className="auth-page">
      <section className="auth-panel" aria-labelledby="sign-in-title">
        <h1 id="sign-in-title">Sign in to Pandar</h1>
        <p>Use your passkey to continue to the dashboard.</p>
        <SignInForm dashboardCallbackUrl={env.dashboardCallbackUrl} />
        <div className="auth-actions">
          <Link className="auth-link" href="/sign-up">
            Create an account
          </Link>
        </div>
      </section>
    </main>
  );
}
