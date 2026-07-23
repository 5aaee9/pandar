import { encodePluginSignInReturnTarget } from "../auth/betterauth/callback-redirect";

type SignInProvider = {
  provider: "clerk" | "logto" | "betterauth" | "none";
  signInUrl: string | null;
};

export function pluginSignInReturnTarget(
  tenant: string | undefined,
  redirectUrl: string,
): string {
  const target = new URL("/plugin-sign-in", "http://pandar.invalid");
  if (tenant) {
    target.searchParams.set("tenant", tenant);
  }
  target.searchParams.set("redirect_url", redirectUrl);
  return `${target.pathname}${target.search}`;
}

export function pluginAuthSignInUrl(
  provider: SignInProvider,
  returnTarget: string,
): string | null {
  if (provider.provider !== "betterauth" || !provider.signInUrl) {
    return provider.signInUrl;
  }

  const signIn = new URL(provider.signInUrl);
  signIn.searchParams.set("return_to", encodePluginSignInReturnTarget(returnTarget));
  return signIn.toString();
}
