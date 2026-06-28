import { cookies, headers } from "next/headers";

type Locale = "en" | "zh";

type AuthMessages = {
  addPasskey: string;
  addingPasskey: string;
  clearingIssuerSession: string;
  continueDashboard: string;
  dashboardTokenEmpty: string;
  dashboardTokenFailed: string;
  email: string;
  issuer: string;
  magicLinkCheckInbox: string;
  magicLinkEmailSent: string;
  magicLinkResend: string;
  magicLinkResendCooldown: string;
  magicLinkSendFailed: string;
  magicLinkSubmit: string;
  magicLinkSentBody: string;
  magicLinkSending: string;
  passkeyAdded: string;
  passkeyAddFailed: string;
  passkeyOptionalIntro: string;
  returnToDashboard: string;
  returningDashboard: string;
  returnsTo: string;
  retrySignOut: string;
  sessionDetails: string;
  sessionLifetime: string;
  skipPasskey: string;
  signIn: string;
  signInFailed: string;
  signInIntro: string;
  signedOut: string;
  signingOut: string;
  signingOutIntro: string;
  signOutWarning: string;
  unableSignIn: string;
  unableSignOut: string;
  upToDuration: (duration: string) => string;
  durationHours: (hours: number) => string;
  durationMinutes: (minutes: number) => string;
};

type SignInMessages = Pick<
  AuthMessages,
  | "email"
  | "magicLinkCheckInbox"
  | "magicLinkEmailSent"
  | "magicLinkResend"
  | "magicLinkResendCooldown"
  | "magicLinkSendFailed"
  | "magicLinkSubmit"
  | "magicLinkSentBody"
  | "magicLinkSending"
  | "signIn"
  | "signInIntro"
  | "unableSignIn"
>;

type CompleteAuthMessages = Pick<
  AuthMessages,
  | "addPasskey"
  | "addingPasskey"
  | "continueDashboard"
  | "dashboardTokenEmpty"
  | "dashboardTokenFailed"
  | "passkeyAdded"
  | "passkeyAddFailed"
  | "passkeyOptionalIntro"
  | "returningDashboard"
  | "skipPasskey"
>;

type SignOutMessages = Pick<
  AuthMessages,
  | "clearingIssuerSession"
  | "returningDashboard"
  | "returnToDashboard"
  | "retrySignOut"
  | "signedOut"
  | "signingOutIntro"
  | "signOutWarning"
  | "unableSignOut"
>;

const messages: Record<Locale, AuthMessages> = {
  en: {
    addPasskey: "Add passkey",
    addingPasskey: "Adding passkey...",
    clearingIssuerSession: "Clearing issuer session",
    continueDashboard: "Continue to dashboard",
    dashboardTokenEmpty: "Dashboard token response was empty",
    dashboardTokenFailed: "Unable to create dashboard token",
    email: "Email",
    issuer: "Issuer",
    magicLinkCheckInbox: "Check your inbox",
    magicLinkEmailSent: "Magic link sent",
    magicLinkResend: "Resend magic link",
    magicLinkResendCooldown: "Resend in {seconds}s",
    magicLinkSendFailed: "Unable to send sign-in link",
    magicLinkSubmit: "Send magic link",
    magicLinkSentBody:
      "If this email can sign in to Pandar, a link will arrive shortly. The message is the same either way to protect account privacy.",
    magicLinkSending: "Sending link...",
    passkeyAdded: "Passkey added",
    passkeyAddFailed:
      "Unable to add a passkey right now. You can continue without one.",
    passkeyOptionalIntro:
      "Add a passkey to make future sign-ins faster on this device. This step is optional.",
    returnToDashboard: "Return to dashboard",
    returningDashboard: "Returning to the Pandar dashboard.",
    returnsTo: "Returns to",
    retrySignOut: "Retry sign-out",
    sessionDetails: "Session details",
    sessionLifetime: "Session lifetime",
    skipPasskey: "Skip",
    signIn: "Sign in to Pandar",
    signInFailed: "Sign-in failed",
    signInIntro:
      "Enter your email and Pandar will send a one-time sign-in link.",
    signedOut: "Signed out",
    signingOut: "Signing out",
    signingOutIntro:
      "Clearing the session from this passkey issuer, then returning you to the Pandar dashboard.",
    signOutWarning: "Sign-out warning",
    unableSignIn: "Unable to sign in",
    unableSignOut: "Unable to sign out",
    upToDuration: (duration) => `Up to ${duration}`,
    durationHours: (hours) => `${hours} hours`,
    durationMinutes: (minutes) => `${minutes} minutes`,
  },
  zh: {
    addPasskey: "添加通行密钥",
    addingPasskey: "正在添加通行密钥...",
    clearingIssuerSession: "正在清除签发器会话",
    continueDashboard: "继续前往控制台",
    dashboardTokenEmpty: "控制台令牌响应为空",
    dashboardTokenFailed: "无法创建控制台令牌",
    email: "邮箱",
    issuer: "签发器",
    magicLinkCheckInbox: "请检查邮箱",
    magicLinkEmailSent: "登录链接已发送",
    magicLinkResend: "重新发送登录链接",
    magicLinkResendCooldown: "{seconds} 秒后可重新发送",
    magicLinkSendFailed: "无法发送登录链接",
    magicLinkSubmit: "发送登录链接",
    magicLinkSentBody:
      "如果此邮箱可以登录 Pandar，登录链接会很快送达。无论账号是否存在，此处都会显示相同提示以保护账号隐私。",
    magicLinkSending: "正在发送链接...",
    passkeyAdded: "通行密钥已添加",
    passkeyAddFailed: "暂时无法添加通行密钥。你可以先跳过并继续。",
    passkeyOptionalIntro:
      "添加通行密钥后，此设备之后登录会更快。此步骤可以跳过。",
    returnToDashboard: "返回控制台",
    returningDashboard: "正在返回 Pandar 控制台。",
    returnsTo: "返回到",
    retrySignOut: "重试退出登录",
    sessionDetails: "会话详情",
    sessionLifetime: "会话有效期",
    skipPasskey: "跳过",
    signIn: "登录 Pandar",
    signInFailed: "登录失败",
    signInIntro: "输入邮箱，Pandar 会发送一次性登录链接。",
    signedOut: "已退出登录",
    signingOut: "正在退出登录",
    signingOutIntro: "正在从此通行密钥签发器清除会话，然后返回 Pandar 控制台。",
    signOutWarning: "退出登录警告",
    unableSignIn: "无法登录",
    unableSignOut: "无法退出登录",
    upToDuration: (duration) => `最长 ${duration}`,
    durationHours: (hours) => `${hours} 小时`,
    durationMinutes: (minutes) => `${minutes} 分钟`,
  },
};

export type {
  AuthMessages,
  CompleteAuthMessages,
  Locale,
  SignInMessages,
  SignOutMessages,
};

export async function getAuthLocale(): Promise<Locale> {
  const cookieStore = await cookies();
  const cookieLocale = cookieStore.get("locale")?.value;
  if (cookieLocale === "zh") {
    return "zh";
  }

  const headerList = await headers();
  return /\bzh(?:\b|[-_])/i.test(headerList.get("accept-language") ?? "")
    ? "zh"
    : "en";
}

export function getAuthMessages(locale: Locale): AuthMessages {
  return messages[locale];
}
