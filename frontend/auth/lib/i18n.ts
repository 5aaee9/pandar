import { cookies, headers } from "next/headers";

type Locale = "en" | "zh";

type AuthMessages = {
  alreadyHaveAccount: string;
  clearingIssuerSession: string;
  createAccount: string;
  createAccountWithPasskey: string;
  createAccountIntro: string;
  dashboardTokenEmpty: string;
  dashboardTokenFailed: string;
  deviceConfirmation: string;
  email: string;
  issuer: string;
  name: string;
  passkeyRegistrationFailed: string;
  passkeySignInFailed: string;
  registerFailed: string;
  returnToDashboard: string;
  returningDashboard: string;
  returnsTo: string;
  retrySignOut: string;
  sessionDetails: string;
  sessionLifetime: string;
  signIn: string;
  signInFailed: string;
  signInIntro: string;
  signInWithPasskey: string;
  signedOut: string;
  signingIn: string;
  signingOut: string;
  signingOutIntro: string;
  signingUp: string;
  signOutWarning: string;
  unableCreateAccount: string;
  unableSignIn: string;
  unableSignOut: string;
  upToDuration: (duration: string) => string;
  durationHours: (hours: number) => string;
  durationMinutes: (minutes: number) => string;
};

type SignInMessages = Pick<
  AuthMessages,
  | "dashboardTokenEmpty"
  | "dashboardTokenFailed"
  | "passkeySignInFailed"
  | "signInFailed"
  | "signingIn"
  | "signInWithPasskey"
  | "unableSignIn"
>;

type SignUpMessages = Pick<
  AuthMessages,
  | "createAccountWithPasskey"
  | "dashboardTokenEmpty"
  | "dashboardTokenFailed"
  | "deviceConfirmation"
  | "email"
  | "name"
  | "passkeyRegistrationFailed"
  | "passkeySignInFailed"
  | "registerFailed"
  | "signingUp"
  | "unableCreateAccount"
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
    alreadyHaveAccount: "Already have an account?",
    clearingIssuerSession: "Clearing issuer session",
    createAccount: "Create your account",
    createAccountWithPasskey: "Create account with passkey",
    createAccountIntro:
      "Register a passkey on this issuer. Pandar uses it to create short-lived dashboard sessions without storing a password.",
    dashboardTokenEmpty: "Dashboard token response was empty",
    dashboardTokenFailed: "Unable to create dashboard token",
    deviceConfirmation:
      "Your browser will ask you to confirm with your device PIN, biometrics, or security key.",
    email: "Email",
    issuer: "Issuer",
    name: "Name",
    passkeyRegistrationFailed: "Passkey registration failed",
    passkeySignInFailed: "Passkey sign-in failed",
    registerFailed: "Account creation failed",
    returnToDashboard: "Return to dashboard",
    returningDashboard: "Returning to the Pandar dashboard.",
    returnsTo: "Returns to",
    retrySignOut: "Retry sign-out",
    sessionDetails: "Session details",
    sessionLifetime: "Session lifetime",
    signIn: "Sign in to Pandar",
    signInFailed: "Sign-in failed",
    signInIntro:
      "Use your passkey with this issuer. After approval, Pandar returns you to the operations dashboard with a short-lived session.",
    signInWithPasskey: "Sign in with passkey",
    signedOut: "Signed out",
    signingIn: "Signing in...",
    signingOut: "Signing out",
    signingOutIntro:
      "Clearing the session from this passkey issuer, then returning you to the Pandar dashboard.",
    signingUp: "Creating account...",
    signOutWarning: "Sign-out warning",
    unableCreateAccount: "Unable to create account",
    unableSignIn: "Unable to sign in",
    unableSignOut: "Unable to sign out",
    upToDuration: (duration) => `Up to ${duration}`,
    durationHours: (hours) => `${hours} hours`,
    durationMinutes: (minutes) => `${minutes} minutes`,
  },
  zh: {
    alreadyHaveAccount: "已有账号？",
    clearingIssuerSession: "正在清除签发器会话",
    createAccount: "创建账号",
    createAccountWithPasskey: "使用通行密钥创建账号",
    createAccountIntro:
      "在此签发器注册通行密钥。Pandar 会用它创建短期控制台会话，不存储密码。",
    dashboardTokenEmpty: "控制台令牌响应为空",
    dashboardTokenFailed: "无法创建控制台令牌",
    deviceConfirmation: "浏览器会要求你使用设备 PIN、生物识别或安全密钥确认。",
    email: "邮箱",
    issuer: "签发器",
    name: "姓名",
    passkeyRegistrationFailed: "通行密钥注册失败",
    passkeySignInFailed: "通行密钥登录失败",
    registerFailed: "账号创建失败",
    returnToDashboard: "返回控制台",
    returningDashboard: "正在返回 Pandar 控制台。",
    returnsTo: "返回到",
    retrySignOut: "重试退出登录",
    sessionDetails: "会话详情",
    sessionLifetime: "会话有效期",
    signIn: "登录 Pandar",
    signInFailed: "登录失败",
    signInIntro:
      "使用此签发器中的通行密钥。确认后，Pandar 会带着短期会话返回运维控制台。",
    signInWithPasskey: "使用通行密钥登录",
    signedOut: "已退出登录",
    signingIn: "正在登录...",
    signingOut: "正在退出登录",
    signingOutIntro: "正在从此通行密钥签发器清除会话，然后返回 Pandar 控制台。",
    signingUp: "正在创建账号...",
    signOutWarning: "退出登录警告",
    unableCreateAccount: "无法创建账号",
    unableSignIn: "无法登录",
    unableSignOut: "无法退出登录",
    upToDuration: (duration) => `最长 ${duration}`,
    durationHours: (hours) => `${hours} 小时`,
    durationMinutes: (minutes) => `${minutes} 分钟`,
  },
};

export type {
  AuthMessages,
  Locale,
  SignInMessages,
  SignOutMessages,
  SignUpMessages,
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
