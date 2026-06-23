import type { NextConfig } from 'next'

const nextConfig: NextConfig = {
  experimental: {
    serverActions: {
      bodySizeLimit: '360mb',
    },
  },
  output: 'standalone',
}

export default nextConfig
