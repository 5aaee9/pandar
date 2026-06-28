import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import Database from "better-sqlite3";
import { decodeProtectedHeader, decodeJwt } from "jose";

const tempDir = await mkdtemp(join(tmpdir(), "pandar-auth-smoke-"));
process.env.PANDAR_AUTH_DATABASE_FILE = join(tempDir, "auth.db");
process.env.PANDAR_AUTH_BASE_URL = "http://127.0.0.1:3001";
process.env.BETTER_AUTH_SECRET = "pandar-auth-smoke-secret-at-least-32-chars";
process.env.PANDAR_AUTH_EMAIL_PROVIDER = "resend";
process.env.PANDAR_AUTH_EMAIL_FROM = "Pandar <smoke@example.com>";
process.env.RESEND_API_KEY = "re_smoke";

try {
  const db = new Database(process.env.PANDAR_AUTH_DATABASE_FILE);
  db.exec(`
    create table jwks (
      id text primary key,
      publicKey text not null,
      privateKey text not null,
      alg text,
      crv text,
      createdAt text not null,
      expiresAt text
    );

    create table user (
      id text primary key,
      name text not null,
      email text not null unique,
      emailVerified integer not null,
      image text,
      createdAt text not null,
      updatedAt text not null
    );
  `);

  const { auth } = await import("../lib/auth.ts");

  const signed = await auth.api.signJWT({
    body: {
      payload: {
        sub: "smoke-user",
        email: "smoke@example.com",
        email_verified: true,
        name: "Smoke User",
        preferred_username: "smoke",
      },
    },
  });

  const header = decodeProtectedHeader(signed.token);
  const payload = decodeJwt(signed.token);
  assert.equal(header.alg, "RS256");
  assert.equal(payload.sub, "smoke-user");
  assert.equal(payload.email, "smoke@example.com");
  assert.equal(payload.email_verified, true);
  assert.equal(payload.name, "Smoke User");
  assert.equal(payload.preferred_username, "smoke");

  const jwks = await auth.api.getJwks();
  const key = jwks.keys.find((candidate) => candidate.kid === header.kid);
  assert.ok(key, `JWKS did not include signed token kid ${header.kid}`);
  assert.equal(key.kty, "RSA");
  assert.equal(key.alg, "RS256");

  const passkeyPlugin = auth.options.plugins.find(
    (plugin) => plugin.id === "passkey",
  );
  assert.ok(passkeyPlugin, "passkey plugin is registered");
  assert.notEqual(passkeyPlugin.options?.registration?.requireSession, false);
  assert.doesNotMatch(
    JSON.stringify(passkeyPlugin.options ?? {}),
    /"requireSession"\s*:\s*false/,
  );
} finally {
  await rm(tempDir, { recursive: true, force: true });
}
