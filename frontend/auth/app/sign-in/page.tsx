import { LoginForm } from "../../components/login-form";
import { getAuthLocale, getAuthMessages } from "../../lib/i18n";

export const dynamic = "force-dynamic";

export default async function SignInPage() {
  const messages = getAuthMessages(await getAuthLocale());

  return (
    <main className="flex min-h-svh flex-col items-center justify-center gap-6 bg-background p-6 md:p-10">
      <div className="w-full max-w-sm">
        <LoginForm
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
            signIn: messages.signIn,
            signInIntro: messages.signInIntro,
            unableSignIn: messages.unableSignIn,
          }}
        />
      </div>
    </main>
  );
}
