import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import type { NextConfig } from "next";

const authRoot = dirname(fileURLToPath(import.meta.url));
const workspaceRoot = join(authRoot, "../..");

const nextConfig: NextConfig = {
  output: "standalone",
  poweredByHeader: false,
  async headers() {
    return [
      {
        source: "/(.*)",
        headers: [
          { key: "X-Content-Type-Options", value: "nosniff" },
          { key: "Referrer-Policy", value: "no-referrer" },
          {
            key: "Permissions-Policy",
            value: "camera=(), microphone=(), geolocation=(), payment=()",
          },
          {
            key: "Strict-Transport-Security",
            value: "max-age=31536000; includeSubDomains",
          },
        ],
      },
    ];
  },
  turbopack: {
    root: workspaceRoot,
  },
};

export default nextConfig;
