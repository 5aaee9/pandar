const returnOrigin = "https://pandar-return.invalid";
const returnTokenPrefix = "v1.";
const maxReturnTokenLength = 4096;

export function normalizePluginReturnTo(
  value: string | string[] | undefined,
): string | null {
  const token = (Array.isArray(value) ? value[0] : value)?.trim();
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
  const candidate = bytes.toString("utf8");
  if (!Buffer.from(candidate, "utf8").equals(bytes)) {
    return null;
  }
  if (
    !candidate.startsWith("/") ||
    candidate.startsWith("//") ||
    candidate.includes("\\")
  ) {
    return null;
  }

  try {
    const target = new URL(candidate, returnOrigin);
    if (
      target.origin !== returnOrigin ||
      target.pathname !== "/plugin-sign-in" ||
      target.hash
    ) {
      return null;
    }
    return `${target.pathname}${target.search}`;
  } catch {
    return null;
  }
}

export function withPluginReturnTo(
  url: string,
  returnTo: string | null,
): string {
  const relative = url.startsWith("/") && !url.startsWith("//");
  const target = new URL(url, returnOrigin);
  target.searchParams.delete("return_to");
  if (returnTo) {
    target.searchParams.set(
      "return_to",
      `${returnTokenPrefix}${Buffer.from(returnTo, "utf8").toString("base64url")}`,
    );
  }

  return relative
    ? `${target.pathname}${target.search}${target.hash}`
    : target.toString();
}
