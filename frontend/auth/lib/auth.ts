import { passkey } from "@better-auth/passkey";
import { betterAuth } from "better-auth";
import { jwt, magicLink } from "better-auth/plugins";
import Database from "better-sqlite3";

import { sendMagicLinkEmail } from "./email.ts";
import { env } from "./env.ts";

function preferredUsername(email: string): string {
  return email.split("@", 1)[0];
}

export const auth = betterAuth({
  database: new Database(env.databaseFile),
  baseURL: env.baseURL,
  basePath: "/api/auth",
  secret: env.secret,
  trustedOrigins: env.trustedOrigins,
  advanced: {
    useSecureCookies: process.env.NODE_ENV === "production",
  },
  plugins: [
    passkey(),
    magicLink({
      expiresIn: env.magicLinkTtlSeconds,
      sendMagicLink: async ({ email, url }) => {
        await sendMagicLinkEmail({
          config: env.email,
          to: email,
          url,
          ttlSeconds: env.magicLinkTtlSeconds,
        });
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
