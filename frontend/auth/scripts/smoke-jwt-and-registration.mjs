import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import Database from "better-sqlite3";
import { decodeProtectedHeader, decodeJwt } from "jose";

const tempDir = await mkdtemp(join(tmpdir(), "pandar-auth-smoke-"));
process.env.PANDAR_AUTH_DATABASE_FILE = join(tempDir, "auth.db");
process.env.PANDAR_AUTH_BASE_URL = "http://127.0.0.1:3001";
process.env.BETTER_AUTH_SECRET = "pandar-auth-smoke-secret-at-least-32-chars";

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
  if (header.alg !== "RS256") {
    throw new Error(`expected JWT header alg RS256, got ${header.alg}`);
  }
  if (payload.sub !== "smoke-user") {
    throw new Error(`expected JWT sub smoke-user, got ${payload.sub}`);
  }

  const jwks = await auth.api.getJwks();
  const key = jwks.keys.find((candidate) => candidate.kid === header.kid);
  if (!key) {
    throw new Error(`JWKS did not include signed token kid ${header.kid}`);
  }
  if (key.kty !== "RSA") {
    throw new Error(`expected JWKS key kty RSA, got ${key.kty}`);
  }
  if (key.alg !== "RS256") {
    throw new Error(`expected JWKS key alg RS256, got ${key.alg}`);
  }

  db.prepare(
    "insert into user (id, name, email, emailVerified, createdAt, updatedAt) values (?, ?, ?, ?, ?, ?)",
  ).run(
    "existing-user",
    "Existing User",
    "victim@example.com",
    1,
    new Date().toISOString(),
    new Date().toISOString(),
  );

  const resolveUser = auth.options.plugins[0].options.registration.resolveUser;
  let reusedExistingUser = false;
  try {
    const result = await resolveUser({
      context: JSON.stringify({
        email: "victim@example.com",
        name: "Attacker",
      }),
      ctx: {
        context: {
          internalAdapter: {
            async findUserByEmail(email) {
              const row = db
                .prepare("select * from user where email = ?")
                .get(email);
              return row
                ? {
                    user: {
                      id: row.id,
                      email: row.email,
                      emailVerified: Boolean(row.emailVerified),
                      name: row.name,
                    },
                    accounts: [],
                  }
                : null;
            },
            async createUser(user) {
              db.prepare(
                "insert into user (id, name, email, emailVerified, createdAt, updatedAt) values (?, ?, ?, ?, ?, ?)",
              ).run(
                "new-user",
                user.name,
                user.email,
                user.emailVerified ? 1 : 0,
                new Date().toISOString(),
                new Date().toISOString(),
              );
              return {
                id: "new-user",
                ...user,
              };
            },
            async updateUser() {
              throw new Error(
                "existing user must not be updated during signup",
              );
            },
          },
        },
      },
    });
    reusedExistingUser = result.id === "existing-user";
  } catch {
    reusedExistingUser = false;
  }

  if (reusedExistingUser) {
    throw new Error("signup resolveUser reused an existing email account");
  }
} finally {
  await rm(tempDir, { recursive: true, force: true });
}
