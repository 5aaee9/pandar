# Pandar Auth Sign-In Render Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix `pandar-auth` `/sign-in` production rendering by removing non-serializable function props from the sign-in Client Component boundary.

**Architecture:** Keep locale message ownership in `frontend/auth/lib/i18n.ts`, but make the countdown copy a string template. Format the countdown inside `frontend/auth/app/sign-in/sign-in-form.tsx`, where the countdown state already exists.

**Tech Stack:** Next.js 16 App Router, React Client Components, TypeScript, Better Auth, Playwright smoke test.

---

### Task 1: Make Sign-In Cooldown Copy Serializable

**Files:**

- Modify: `frontend/auth/lib/i18n.ts`
- Modify: `frontend/auth/app/sign-in/sign-in-form.tsx`
- Inspect: `frontend/auth/app/sign-in/page.tsx`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Confirm or change the message type**

In `frontend/auth/lib/i18n.ts`, confirm the type is:

```ts
magicLinkResendCooldown: string;
```

If it is still:

```ts
magicLinkResendCooldown: (seconds: number) => string;
```

change it to:

```ts
magicLinkResendCooldown: string;
```

- [ ] **Step 2: Confirm or change localized cooldown messages to templates**

In `frontend/auth/lib/i18n.ts`, confirm the English message is:

```ts
magicLinkResendCooldown: "Resend in {seconds}s",
```

and the Chinese message is:

```ts
magicLinkResendCooldown: "{seconds} 秒后可重新发送",
```

If either locale still uses a function, replace it with the matching string template above.

- [ ] **Step 3: Confirm or add client-side template formatting**

In `frontend/auth/app/sign-in/sign-in-form.tsx`, confirm this helper exists:

```ts
function formatCooldown(template: string, seconds: number): string {
  return template.replace("{seconds}", String(seconds));
}
```

Confirm the button label branch uses:

```tsx
formatCooldown(messages.magicLinkResendCooldown, cooldown);
```

If it still uses:

```tsx
messages.magicLinkResendCooldown(cooldown);
```

replace it with:

```tsx
formatCooldown(messages.magicLinkResendCooldown, cooldown);
```

- [ ] **Step 4: Confirm the Server Component boundary is narrow**

In `frontend/auth/app/sign-in/page.tsx`, confirm `SignInForm` receives an object containing only these serializable fields:

```tsx
<SignInForm
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
    unableSignIn: messages.unableSignIn,
  }}
/>
```

Do not pass the full `messages` object to `SignInForm`, because `AuthMessages` still contains server-only formatter functions used by `AuthSessionContext`.

- [ ] **Step 5: Update or confirm the roadmap**

Add a `docs/roadmap.md` completed bullet recording that the standalone Better Auth issuer sign-in page no longer passes non-serializable cooldown functions across the Server Component boundary.

### Task 2: Verify Production-Equivalent Local Behavior

**Files:**

- Read: `frontend/auth/package.json`
- Read: `frontend/auth/lib/env.ts`

- [ ] **Step 1: Build the auth app**

Run:

```bash
npm run build
```

from `frontend/auth`.

Expected: exit code 0 and route list includes `/sign-in`.

- [ ] **Step 2: Start the auth app with production-shaped env in the background**

Run from `frontend/auth`:

```bash
PANDAR_AUTH_BASE_URL=http://127.0.0.1:3001 \
PANDAR_AUTH_TRUSTED_ORIGINS=http://127.0.0.1:3000 \
PANDAR_AUTH_DASHBOARD_CALLBACK_URL=http://127.0.0.1:3000/auth/betterauth/callback \
PANDAR_AUTH_DASHBOARD_SIGN_OUT_URL=http://127.0.0.1:3000/auth/betterauth/sign-out \
PANDAR_AUTH_DATABASE_FILE=/tmp/pandar-auth-local-smoke.db \
PANDAR_AUTH_EMAIL_PROVIDER=resend \
PANDAR_AUTH_EMAIL_FROM='Pandar <auth@example.invalid>' \
PANDAR_AUTH_EMAIL_BRAND_NAME=Pandar \
PANDAR_AUTH_JWT_MAX_AGE_SECONDS=43200 \
PANDAR_AUTH_MAGIC_LINK_TTL_SECONDS=1800 \
BETTER_AUTH_SECRET=local-smoke-secret-at-least-32-chars \
RESEND_API_KEY=local-smoke-placeholder \
HOSTNAME=127.0.0.1 \
PORT=3001 \
nohup npm run start >/tmp/pandar-auth-local-smoke.log 2>&1 &
echo $! >/tmp/pandar-auth-local-smoke.pid
```

Poll until the server responds:

```bash
for attempt in $(seq 1 30); do
  if curl -fsS http://127.0.0.1:3001/sign-in >/tmp/pandar-auth-local-smoke.html; then
    break
  fi
  sleep 1
done
curl -fsS http://127.0.0.1:3001/sign-in >/tmp/pandar-auth-local-smoke.html
```

Expected: server reports ready on `http://127.0.0.1:3001`.

- [ ] **Step 3: Check HTTP status**

Run:

```bash
curl -i --max-time 20 http://127.0.0.1:3001/sign-in
```

Expected: `HTTP/1.1 200 OK`, not a Next.js production error page.

- [ ] **Step 4: Check with Playwright**

Run this temporary Playwright smoke script from the repository root:

```bash
PLAYWRIGHT_TMP="$(mktemp -d)"
cd "$PLAYWRIGHT_TMP"
npm init -y >/dev/null
npm install playwright-core@1.60.0 >/dev/null
PLAYWRIGHT_BROWSERS_PATH="$(nix eval --raw nixpkgs#playwright-driver.browsers)"
cat >pandar-auth-sign-in-smoke.js <<'EOF'
const { chromium } = require("playwright-core");

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage();
  const consoleErrors = [];
  page.on("console", (message) => {
    if (message.type() === "error") {
      consoleErrors.push(message.text());
    }
  });

  const response = await page.goto("http://127.0.0.1:3001/sign-in", {
    waitUntil: "networkidle",
  });
  if (!response || response.status() >= 500) {
    throw new Error(`unexpected status ${response?.status()}`);
  }

  await page.getByRole("heading", { name: "Sign in to Pandar" }).waitFor();
  await page.getByRole("textbox", { name: "Email" }).waitFor();

  const boundaryErrors = consoleErrors.filter((message) =>
    message.includes("Functions cannot be passed directly to Client Components"),
  );
  if (boundaryErrors.length > 0) {
    throw new Error(boundaryErrors.join("\n"));
  }

  await browser.close();
  console.log("Playwright sign-in smoke passed");
})();
EOF
PLAYWRIGHT_BROWSERS_PATH="$PLAYWRIGHT_BROWSERS_PATH" node pandar-auth-sign-in-smoke.js
cd -
```

Expected: Playwright exits 0 and confirms the visible sign-in UI.

- [ ] **Step 5: Stop the local smoke server**

Run:

```bash
kill "$(cat /tmp/pandar-auth-local-smoke.pid)"
rm -f /tmp/pandar-auth-local-smoke.pid
```

Expected: local smoke server stops cleanly.

### Task 3: Review, Commit, And Push

**Files:**

- Review: full git diff

- [ ] **Step 1: Run final verification**

Run:

```bash
npm run build
```

from `frontend/auth`, then repeat the local server and Playwright smoke test.

Expected: all commands exit 0.

- [ ] **Step 2: Inspect diff**

Run:

```bash
git status --short
git diff
```

Expected: only the auth sign-in fix, SDD docs, and roadmap update are changed.

- [ ] **Step 3: Commit with Lore protocol**

Stage only the intended files:

```bash
git add \
  docs/roadmap.md \
  docs/superpowers/plans/2026-06-29-pandar-auth-sign-in-render.md \
  docs/superpowers/specs/2026-06-29-pandar-auth-sign-in-render-design.md \
  frontend/auth/app/sign-in/sign-in-form.tsx \
  frontend/auth/lib/i18n.ts
```

Use a commit message with an intent line, useful decision trailers, and `Tested:` entries for the build and Playwright smoke.

- [ ] **Step 4: Push current branch**

Run:

```bash
git push
```

If no upstream is configured, push with:

```bash
git push -u origin HEAD
```
