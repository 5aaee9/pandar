import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";

import {
  magicLinkHtml,
  magicLinkSubject,
  magicLinkText,
} from "../lib/email.ts";

const node = process.execPath;
const baseEnv = {
  PATH: process.env.PATH,
  HOME: process.env.HOME,
  NODE_ENV: "production",
  BETTER_AUTH_SECRET: "pandar-auth-smoke-secret-at-least-32-chars",
};

const resendEnv = {
  PANDAR_AUTH_EMAIL_PROVIDER: "resend",
  PANDAR_AUTH_EMAIL_FROM: "Pandar <auth@example.invalid>",
  RESEND_API_KEY: "resend_test_key",
};

const smtpEnv = {
  PANDAR_AUTH_EMAIL_PROVIDER: "smtp",
  PANDAR_AUTH_EMAIL_FROM: "Pandar <auth@example.invalid>",
  PANDAR_AUTH_SMTP_HOST: "smtp.example.invalid",
  PANDAR_AUTH_SMTP_PORT: "2525",
  PANDAR_AUTH_SMTP_USERNAME: "smtp-user",
  PANDAR_AUTH_SMTP_PASSWORD: "smtp-password",
  PANDAR_AUTH_SMTP_TLS: "starttls",
};

assert.equal(magicLinkSubject("Pandar"), "Sign in to Pandar");

const text = magicLinkText("Pandar", "https://auth.example/link", 1_800);
assert.match(text, /https:\/\/auth\.example\/link/);
assert.match(text, /30 minutes/);

const html = magicLinkHtml(
  "Pandar & <Auth>",
  'https://auth.example/link?token=<secret>&next="dashboard"',
  1_800,
);
assert.match(html, /Pandar &amp; &lt;Auth&gt;/);
assert.match(
  html,
  /https:\/\/auth\.example\/link\?token=&lt;secret&gt;&amp;next=&quot;dashboard&quot;/,
);

assertEnv({
  name: "resend runtime env parses",
  env: resendEnv,
  script: `
    const { env } = await import("./lib/env.ts");
    assert.equal(env.email.provider, "resend");
    assert.equal(env.email.from, "Pandar <auth@example.invalid>");
    assert.equal(env.email.brandName, "Pandar");
    assert.equal(env.email.apiKey, "resend_test_key");
    assert.equal(env.magicLinkTtlSeconds, 1800);
  `,
});

assertEnv({
  name: "smtp runtime env parses",
  env: smtpEnv,
  script: `
    const { env } = await import("./lib/env.ts");
    assert.equal(env.email.provider, "smtp");
    assert.equal(env.email.host, "smtp.example.invalid");
    assert.equal(env.email.port, 2525);
    assert.equal(env.email.username, "smtp-user");
    assert.equal(env.email.password, "smtp-password");
    assert.equal(env.email.tls, "starttls");
  `,
});

assertEnv({
  name: "build env uses dummy email config",
  env: {
    npm_lifecycle_event: "build",
    PANDAR_AUTH_EMAIL_BRAND_NAME: "Build Brand",
  },
  script: `
    const { env } = await import("./lib/env.ts");
    assert.equal(env.email.provider, "resend");
    assert.equal(env.email.from, "Pandar <build@example.invalid>");
    assert.equal(env.email.brandName, "Build Brand");
    assert.equal(env.email.apiKey, "build-resend-api-key");
  `,
});

assertEnvThrows({
  name: "runtime missing provider throws",
  env: {},
  message: "PANDAR_AUTH_EMAIL_PROVIDER must be resend or smtp",
});

assertEnvThrows({
  name: "runtime invalid provider throws",
  env: { PANDAR_AUTH_EMAIL_PROVIDER: "bogus" },
  message: "PANDAR_AUTH_EMAIL_PROVIDER must be resend or smtp",
});

assertEnvThrows({
  name: "build invalid provider throws",
  env: {
    npm_lifecycle_event: "build",
    PANDAR_AUTH_EMAIL_PROVIDER: "bogus",
  },
  message: "PANDAR_AUTH_EMAIL_PROVIDER must be resend or smtp",
});

assertEnvThrows({
  name: "invalid magic link TTL throws",
  env: {
    ...resendEnv,
    PANDAR_AUTH_MAGIC_LINK_TTL_SECONDS: "0",
  },
  message: "PANDAR_AUTH_MAGIC_LINK_TTL_SECONDS must be a positive integer",
});

assertEnvThrows({
  name: "invalid SMTP TLS throws",
  env: {
    ...smtpEnv,
    PANDAR_AUTH_SMTP_TLS: "ssl",
  },
  message: "PANDAR_AUTH_SMTP_TLS must be starttls, tls, or none",
});

assertEnvThrows({
  name: "runtime missing Resend API key throws",
  env: {
    PANDAR_AUTH_EMAIL_PROVIDER: "resend",
    PANDAR_AUTH_EMAIL_FROM: "Pandar <auth@example.invalid>",
  },
  message: "RESEND_API_KEY is required",
});

assertEnvThrows({
  name: "runtime missing SMTP port throws",
  env: {
    ...smtpEnv,
    PANDAR_AUTH_SMTP_PORT: "",
  },
  message: "PANDAR_AUTH_SMTP_PORT is required",
});

assertEnvThrows({
  name: "runtime missing SMTP password throws",
  env: {
    ...smtpEnv,
    PANDAR_AUTH_SMTP_PASSWORD: "",
  },
  message: "PANDAR_AUTH_SMTP_PASSWORD is required",
});

await assertSignUpRedirect();

console.log("smoke-email-env passed");

async function assertSignUpRedirect() {
  try {
    const { default: SignUpPage } = await import("../app/sign-up/page.tsx");
    assert.throws(
      () => SignUpPage(),
      (error) =>
        error instanceof Error &&
        typeof error.digest === "string" &&
        error.digest.includes("NEXT_REDIRECT") &&
        error.digest.includes("/sign-in"),
      "sign-up page redirects to /sign-in",
    );
    return;
  } catch (error) {
    if (
      !(error instanceof TypeError) ||
      error.code !== "ERR_UNKNOWN_FILE_EXTENSION"
    ) {
      throw error;
    }

    console.warn(
      "Direct sign-up route import skipped: Node strip-types cannot load .tsx route modules.",
    );
  }

  const source = await readFile(
    new URL("../app/sign-up/page.tsx", import.meta.url),
    "utf8",
  );
  assert.match(source, /redirect\(["']\/sign-in["']\)/);
}

function assertEnv({ name, env, script }) {
  const result = runEnvScript(env, script);
  assert.equal(
    result.status,
    0,
    `${name} failed\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
  );
}

function assertEnvThrows({ name, env, message }) {
  const result = runEnvScript(
    env,
    `
      await import("./lib/env.ts");
    `,
  );
  assert.notEqual(result.status, 0, `${name} unexpectedly passed`);
  assert.match(result.stderr, new RegExp(escapeRegExp(message)), name);
}

function runEnvScript(env, script) {
  return spawnSync(
    node,
    [
      "--experimental-strip-types",
      "--input-type=module",
      "-e",
      `
        import assert from "node:assert/strict";
        ${script}
      `,
    ],
    {
      cwd: new URL("..", import.meta.url),
      env: {
        ...baseEnv,
        ...env,
      },
      encoding: "utf8",
    },
  );
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
