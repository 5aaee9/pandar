export function safeSignOutRedirectTarget(
  next: string | null,
  signInUrl: string | null,
) {
  if (!next || !signInUrl) {
    return "/";
  }
  return next === signInUrl ? next : "/";
}
