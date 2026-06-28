import { passkey } from "@better-auth/passkey";
import { betterAuth } from "better-auth";
import { jwt } from "better-auth/plugins";
import Database from "better-sqlite3";

import { env } from "./env.ts";

type BetterAuthUser = {
  id: string;
  email: string;
  emailVerified: boolean;
  name: string;
};

type InternalAdapter = {
  findUserByEmail(
    email: string,
  ): Promise<{ user: BetterAuthUser; accounts: unknown[] } | null>;
  createUser(user: {
    email: string;
    emailVerified: boolean;
    name: string;
  }): Promise<BetterAuthUser>;
  updateUser(
    id: string,
    user: Partial<Pick<BetterAuthUser, "emailVerified" | "name">>,
  ): Promise<BetterAuthUser>;
};

type ResolveUserContext = {
  internalAdapter: InternalAdapter;
};

function normalizeEmail(email: string): string {
  const normalized = email.trim().toLowerCase();
  if (!normalized || !normalized.includes("@")) {
    throw new Error("A valid email address is required");
  }

  return normalized;
}

function normalizeName(name: unknown, email: string): string {
  if (typeof name === "string" && name.trim()) {
    return name.trim();
  }

  return email.split("@", 1)[0];
}

function parseRegistrationContext(context: string | null | undefined): {
  email: string;
  name: string;
} {
  if (!context) {
    throw new Error("Passkey registration context is required");
  }

  const parsed = JSON.parse(context) as { email?: unknown; name?: unknown };
  if (typeof parsed.email !== "string") {
    throw new Error("Passkey registration context email is required");
  }

  const email = normalizeEmail(parsed.email);
  return {
    email,
    name: normalizeName(parsed.name, email),
  };
}

function preferredUsername(email: string): string {
  return email.split("@", 1)[0];
}

export const auth = betterAuth({
  database: new Database(env.databaseFile),
  baseURL: env.baseURL,
  basePath: "/api/auth",
  secret: env.secret,
  trustedOrigins: env.trustedOrigins,
  plugins: [
    passkey({
      registration: {
        requireSession: false,
        resolveUser: async ({ ctx, context }) => {
          const { email, name } = parseRegistrationContext(context);
          const { internalAdapter } = ctx.context as unknown as ResolveUserContext;

          const existingUser = await internalAdapter.findUserByEmail(email);
          const user =
            existingUser?.user ??
            (await internalAdapter.createUser({
              email,
              name,
              emailVerified: true,
            }));

          const verifiedUser = user.emailVerified
            ? user
            : await internalAdapter.updateUser(user.id, {
                emailVerified: true,
                name,
              });

          return {
            id: verifiedUser.id,
            name: verifiedUser.name || name,
            displayName: verifiedUser.name || name,
          };
        },
      },
    }),
    jwt({
      jwks: {
        keyPairConfig: {
          alg: "RS256",
        },
        jwksPath: "/jwks",
      },
      jwt: {
        issuer: env.baseURL,
        audience: env.baseURL,
        expirationTime: `${env.jwtMaxAgeSeconds}s`,
        definePayload: ({ user }) => ({
          email: user.email,
          email_verified: user.emailVerified,
          name: user.name,
          preferred_username: preferredUsername(user.email),
        }),
      },
    }),
  ],
});
