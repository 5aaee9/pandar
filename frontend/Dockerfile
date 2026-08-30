FROM node:24-bookworm-slim@sha256:6f7b03f7c2c8e2e784dcf9295400527b9b1270fd37b7e9a7285cf83b6951452d AS deps

WORKDIR /app
COPY package.json package-lock.json ./
COPY frontend/package.json ./frontend/package.json
COPY frontend/auth/package.json ./frontend/auth/package.json
COPY frontend/plugin-local/package.json ./frontend/plugin-local/package.json
RUN npm ci

FROM node:24-bookworm-slim@sha256:6f7b03f7c2c8e2e784dcf9295400527b9b1270fd37b7e9a7285cf83b6951452d AS builder

WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY --from=deps /app/package.json ./package.json
COPY --from=deps /app/package-lock.json ./package-lock.json
COPY --from=deps /app/frontend/package.json ./frontend/package.json
COPY --from=deps /app/frontend/auth/package.json ./frontend/auth/package.json
COPY --from=deps /app/frontend/plugin-local/package.json ./frontend/plugin-local/package.json
COPY frontend ./frontend
COPY contracts ./contracts
RUN npm run build:web

FROM node:24-bookworm-slim@sha256:6f7b03f7c2c8e2e784dcf9295400527b9b1270fd37b7e9a7285cf83b6951452d AS runner

WORKDIR /app
ENV NODE_ENV=production
ENV PORT=3000

COPY --from=builder /app/frontend/.next/standalone ./
COPY --from=builder /app/frontend/.next/static ./frontend/.next/static
COPY --from=builder /app/frontend/public ./frontend/public

USER 1000:1000
EXPOSE 3000
CMD ["node", "frontend/server.js"]
