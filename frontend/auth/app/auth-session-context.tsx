import type { AuthMessages } from "../lib/i18n";

type AuthSessionContextProps = {
  dashboardCallbackUrl: string;
  issuerUrl: string;
  jwtMaxAgeSeconds: number;
  messages: AuthMessages;
};

function displayHost(value: string): string {
  try {
    return new URL(value).host;
  } catch {
    return value;
  }
}

function displayDuration(seconds: number, messages: AuthMessages): string {
  const hours = Math.round(seconds / 3600);
  return hours >= 1
    ? messages.durationHours(hours)
    : messages.durationMinutes(Math.round(seconds / 60));
}

export function AuthSessionContext({
  dashboardCallbackUrl,
  issuerUrl,
  jwtMaxAgeSeconds,
  messages,
}: AuthSessionContextProps) {
  return (
    <dl className="auth-context" aria-label={messages.sessionDetails}>
      <div>
        <dt>{messages.issuer}</dt>
        <dd>{displayHost(issuerUrl)}</dd>
      </div>
      <div>
        <dt>{messages.returnsTo}</dt>
        <dd>{displayHost(dashboardCallbackUrl)}</dd>
      </div>
      <div>
        <dt>{messages.sessionLifetime}</dt>
        <dd>{messages.upToDuration(displayDuration(jwtMaxAgeSeconds, messages))}</dd>
      </div>
    </dl>
  );
}
