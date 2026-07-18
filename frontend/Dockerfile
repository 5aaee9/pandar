FROM node:24-bookworm-slim AS deps

WORKDIR /app
COPY package.json package-lock.json ./
COPY frontend/package.json ./frontend/package.json
COPY frontend/auth/package.json ./frontend/auth/package.json
COPY frontend/plugin-local/package.json ./frontend/plugin-local/package.json
RUN npm ci

FROM node:24-bookworm-slim AS builder

WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY --from=deps /app/package.json ./package.json
COPY --from=deps /app/package-lock.json ./package-lock.json
COPY --from=deps /app/frontend/package.json ./frontend/package.json
COPY --from=deps /app/frontend/auth/package.json ./frontend/auth/package.json
COPY --from=deps /app/frontend/plugin-local/package.json ./frontend/plugin-local/package.json
COPY frontend ./frontend
RUN npm run build:web

FROM node:24-bookworm-slim AS runner

WORKDIR /app
ENV NODE_ENV=production
ENV PORT=3000

COPY --chown=1000:1000 --from=builder /app/frontend/.next/standalone ./
COPY --chown=1000:1000 --from=builder /app/frontend/.next/static ./frontend/.next/static
COPY --chown=1000:1000 --from=builder /app/frontend/public ./frontend/public

USER 1000:1000
EXPOSE 3000
CMD ["node", "frontend/server.js"]
