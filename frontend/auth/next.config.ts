import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import type { NextConfig } from "next";

const authRoot = dirname(fileURLToPath(import.meta.url));
const workspaceRoot = join(authRoot, "../..");

const nextConfig: NextConfig = {
  output: "standalone",
  turbopack: {
    root: workspaceRoot,
  },
};

export default nextConfig;
