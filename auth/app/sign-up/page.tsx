import Link from "next/link";

import { env } from "../../lib/env";
import { SignUpForm } from "./sign-up-form";

export const dynamic = "force-dynamic";

export default function SignUpPage() {
  return (
    <main className="auth-page">
      <section className="auth-panel" aria-labelledby="sign-up-title">
        <h1 id="sign-up-title">Create your account</h1>
        <p>Register a passkey for passwordless access to Pandar.</p>
        <SignUpForm dashboardCallbackUrl={env.dashboardCallbackUrl} />
        <div className="auth-actions">
          <Link className="auth-link" href="/sign-in">
            Already have an account?
          </Link>
        </div>
      </section>
    </main>
  );
}
