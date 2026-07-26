const returnTokenPrefix = "v1.";
const maxReturnTokenLength = 4096;

export function encodePluginSignInReturnTarget(target: string): string {
  return `${returnTokenPrefix}${Buffer.from(target, "utf8").toString("base64url")}`;
}

export function decodePluginSignInReturnTarget(
  value: string | null | undefined,
): string | null {
  const token = value?.trim();
  if (
    !token?.startsWith(returnTokenPrefix) ||
    token.length > maxReturnTokenLength
  ) {
    return null;
  }

  const encoded = token.slice(returnTokenPrefix.length);
  if (!encoded || !/^[A-Za-z0-9_-]+$/.test(encoded)) {
    return null;
  }

  const bytes = Buffer.from(encoded, "base64url");
  if (bytes.toString("base64url") !== encoded) {
    return null;
  }
  const target = bytes.toString("utf8");
  if (!Buffer.from(target, "utf8").equals(bytes)) {
    return null;
  }
  if (
    !target.startsWith("/") ||
    target.startsWith("//") ||
    target.includes("\\")
  ) {
    return null;
  }

  try {
    const parsed = new URL(target, "http://pandar.invalid");
    if (
      parsed.origin !== "http://pandar.invalid" ||
      parsed.pathname !== "/plugin-sign-in" ||
      parsed.hash
    ) {
      return null;
    }
    return `${parsed.pathname}${parsed.search}`;
  } catch {
    return null;
  }
}

export function betterAuthCallbackTarget(requestUrl: string) {
  return safePluginReturnTarget(new URL(requestUrl));
}

function safePluginReturnTarget(request: URL): string {
  return (
    decodePluginSignInReturnTarget(request.searchParams.get("return_to")) ?? "/"
  );
}

export function dashboardCallbackRedirectUrl(
  target: string,
  requestUrl: string,
  appBaseUrl = process.env.APP_BASE_URL,
): URL {
  const publicBaseUrl = appBaseUrl?.trim();
  return new URL(target, publicBaseUrl || requestUrl);
}
