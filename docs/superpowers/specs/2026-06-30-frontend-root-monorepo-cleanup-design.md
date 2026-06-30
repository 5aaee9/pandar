# Frontend Root Monorepo Cleanup Design

## Goal

Move frontend package management to the repository root and remove moderate duplicate frontend code without changing runtime behavior. The root npm workspace becomes the single source of truth for Node dependency resolution across `pandar-web`, `pandar-auth`, and `pandar-plugin-local`.

## Context

The repository currently has a Rust workspace at the root and three frontend package surfaces under `frontend/`:

- `frontend/` is the `pandar-web` Next.js app and currently owns `frontend/package-lock.json`.
- `frontend/auth/` is the `pandar-auth` Next.js Better Auth issuer and currently owns `frontend/auth/package-lock.json`.
- `frontend/plugin-local/` is an npm workspace of `frontend/` and builds static plugin assets.

This creates two dependency islands. Shared packages such as `next`, `react`, `react-dom`, `typescript`, Tailwind/PostCSS, `class-variance-authority`, `clsx`, `lucide-react`, `tailwind-merge`, and `vitest` are declared separately, sometimes with different versions or range styles. `frontend/lib/utils.ts` and `frontend/auth/lib/utils.ts` are byte-for-byte duplicates. Some shadcn-style primitives overlap between `frontend/components/ui` and `frontend/auth/components/ui`, but their component contracts and visual variants differ.

## Architecture

Add a root `package.json` with npm workspaces:

- `frontend`
- `frontend/auth`
- `frontend/plugin-local`

Generate one root `package-lock.json` and remove the two nested lockfiles. Keep each package's existing `name`, scripts, and application directory. The root package is private and provides orchestration scripts for common tasks, for example `build:web`, `build:auth`, `build:plugin-local`, `test:web`, and `test:auth`, implemented with npm workspace commands.

Remove the existing nested workspace declaration from `frontend/package.json`. After the migration, only the repository-root `package.json` declares workspaces; `frontend/package.json`, `frontend/auth/package.json`, and `frontend/plugin-local/package.json` are ordinary workspace packages. Change `frontend/package.json`'s own `build` script to build only the Next.js dashboard app. The root `build:web` script is responsible for running `build:plugin-local` before building `pandar-web`, preserving the current asset-generation behavior without nested workspaces.

Align overlapping dependency versions exactly where the apps intentionally share the same runtime or toolchain. For packages that appear in both `frontend/package.json` and `frontend/auth/package.json`, use exact matching versions for this list: `next`, `react`, `react-dom`, `typescript`, `@types/node`, `@types/react`, `@types/react-dom`, `class-variance-authority`, `clsx`, `lucide-react`, `tailwind-merge`, and `vitest`. For Tailwind/PostCSS, align `tailwindcss` and `@tailwindcss/postcss` to the newer existing auth toolchain version if both apps build cleanly; keep an app-local exception only if build, runtime, or visual verification shows a regression. Preserve auth-only dependencies (`better-auth`, `auth`, `@better-auth/passkey`, `better-sqlite3`, `nodemailer`, Radix packages, auth-only types) in `frontend/auth/package.json`. Preserve dashboard-only dependencies (`next-intl`, `zustand`, `@base-ui/react`, `shadcn`, `tw-animate-css`, Testing Library packages, `jsdom`) in `frontend/package.json`.

Do not introduce a shared frontend package in this pass. It would be cleaner for broader reuse, but it would add TypeScript export wiring, Next transpilation concerns, and packaging churn that are not needed for the requested cleanup.

## Package And Build Integration

Nix and Docker currently assume app-local lockfiles. They must be updated to use the root npm workspace while still producing separate `pandar-web` and `pandar-auth` outputs.

For Nix, replace the two app-local npm sources with a root-workspace source that includes exactly the files needed for npm workspace resolution and the two app builds:

- root `package.json` and root `package-lock.json`
- `frontend/` excluding `frontend/auth/`, `.next`, `node_modules`, and other generated output
- `frontend/auth/` excluding `.next`, `node_modules`, and local generated output
- `frontend/plugin-local/` including checked-in `dist/` assets because Rust packaging uses them

`pandar-web` and `pandar-auth` can share that same root-workspace source, but their Nix phases must build only the selected workspace. The implementation must run explicit npm workspace commands or custom phases, for example installing from the root lockfile and building `--workspace pandar-web` or `--workspace pandar-auth`, rather than assuming `buildNpmPackage` will infer the intended workspace. Regenerate both fixed-output hashes: the existing `pandar-web` `npmDepsHash` and `pandar-auth` `npmDepsHash` are invalid once the root lockfile replaces nested lockfiles.

`pandar-web` must not bundle `pandar-auth` server code into its installed output. Its install phase should continue to copy only the `pandar-web` standalone server, static assets, and public assets from the web workspace build output.

`pandar-auth` must retain the native `better-sqlite3` build inputs and runtime library path. Its install phase must continue to create `$out/share/pandar-auth/migrate-src` so `pandar-auth-migrate` can run after installation. Because `frontend/auth/package-lock.json` and auth-local `node_modules` are removed, the installed layout must be explicit:

- `$out/share/pandar-auth/migrate-src/package.json` is copied from `frontend/auth/package.json`.
- `$out/share/pandar-auth/migrate-src/tsconfig.json` is copied from `frontend/auth/tsconfig.json`.
- `$out/share/pandar-auth/migrate-src/lib/` and `$out/share/pandar-auth/migrate-src/scripts/` are copied from `frontend/auth/lib/` and `frontend/auth/scripts/`.
- `$out/share/pandar-auth/migrate-src/node_modules/` contains the root-workspace install layout needed to resolve `auth`, `better-auth`, `better-sqlite3`, and their transitive dependencies from `migrate-src`.
- `$out/share/pandar-auth/lib/utils.ts` is copied from `frontend/lib/utils.ts` so `migrate-src/lib/utils.ts` can resolve its `../../lib/utils` re-export target.

The Nix auth smoke coverage must include a command equivalent to `cd $out/share/pandar-auth/migrate-src && node --experimental-strip-types -e 'await import("./lib/utils.ts")'` in addition to the existing migration/JWT smoke paths, so the cross-app re-export is proven from the installed layout.

For Docker, update every existing web image build path. The GitHub workflow `.github/workflows/docker.yml` currently uses `context: frontend` and `dockerfile: frontend/Dockerfile` for the `web` matrix entry. The local compose files `docker-compose.postgres.yml` and `docker-compose.sqlite.yml` currently use `context: frontend` and `dockerfile: Dockerfile` for `pandar-web`. The root workspace requires changing each web build context to the repository root and using `frontend/Dockerfile`. Update `frontend/Dockerfile` so its dependency stage copies root `package.json`, root `package-lock.json`, `frontend/package.json`, `frontend/auth/package.json`, and `frontend/plugin-local/package.json`, runs `npm ci` from the root context, builds only the `pandar-web` workspace, and copies only the `pandar-web` standalone output into the runtime image.

Documentation and scripts should point developers to root npm commands while keeping workspace-specific commands usable when npm supports them.

Update the root `.gitignore` for the new package-manager root. It currently ignores frontend-local generated directories; after the migration it must also ignore root `node_modules/` and root `.next/` if any workspace command creates them.

## Duplicate-Code Cleanup

Use a moderate DRY pass:

- Replace the duplicated `frontend/auth/lib/utils.ts` implementation with a thin re-export from the dashboard utility module: `export { cn } from "../../lib/utils";`. This keeps the auth app's existing `@/lib/utils` imports working while storing the helper implementation in one file. It is intentionally a small monorepo-local coupling instead of a new shared package.
- Remove duplicated package-manager metadata and lockfile state by consolidating to the root lockfile.
- Normalize config drift where the values are meant to be the same, such as shared dependency versions and app toolchain versions.
- Update `README.md` and `docs/development.md` so developer-facing npm commands use the new root workspace scripts, including the self-hosted Better Auth issuer development block that currently starts from `cd frontend/auth` and `npm install`. Update `docs/roadmap.md` after code changes, as required by the repository instructions.
- Do not force the auth shadcn primitives to import dashboard primitives in this pass. `Button`, `Input`, and `Label` are similar but not equivalent: the dashboard uses Base UI contracts for some primitives, while auth uses Radix/native contracts and different sizing/variant classes. Collapsing them would be a visual and behavioral refactor, not just duplicate cleanup.

## Data Flow And Runtime Behavior

No backend API, database, auth, WebSocket, route, or UI workflow changes are intended. Runtime behavior remains:

- `pandar-web` serves the dashboard app from `frontend/`.
- `pandar-auth` serves the Better Auth issuer from `frontend/auth/`.
- `pandar-plugin-local` builds static plugin-local assets consumed by Rust packaging.

The change is limited to dependency resolution, build orchestration, and small duplicate cleanup.

## Error Handling

No new runtime error boundary is required. Build-time errors should fail fast through npm, Next.js, Vitest, Docker, or Nix. Native dependency failures for `pandar-auth` should retain their current Nix build inputs and library path rather than being hidden behind fallback logic.

## Acceptance Criteria

The implementation is accepted only when all of these are true or the exact blocker is recorded:

- The repository root contains a private `package.json` with workspaces for `frontend`, `frontend/auth`, and `frontend/plugin-local`.
- The repository root contains the only npm lockfile. `frontend/package-lock.json` and `frontend/auth/package-lock.json` are deleted.
- `frontend/package.json` no longer declares `workspaces`.
- Shared dependency versions listed in the Architecture section are exact and aligned unless build, runtime, or visual verification proves an app-local exception is required. Any exception must be recorded in `docs/roadmap.md` with the package name, app, retained version, and verification reason. Tailwind/PostCSS must not be downgraded solely to match the dashboard app.
- Root `.gitignore` ignores root `node_modules/` and root `.next/` generated output.
- Root scripts exist and run these workspace commands: `test:web`, `test:auth`, `build:plugin-local`, `build:web`, and `build:auth`.
- `frontend/package.json`'s `build` script builds only the dashboard Next.js app; root `build:web` runs plugin-local first and then the dashboard build.
- Nix builds for `pandar-web` and `pandar-auth` use the root lockfile, build the selected workspace, install the same runtime artifacts as before, and have refreshed `npmDepsHash` values.
- `pandar-auth`'s installed `migrate-src` can resolve the `frontend/auth/lib/utils.ts` re-export target if any migration or smoke import reaches it.
- The Docker web build uses the repository root build context in `.github/workflows/docker.yml`, `docker-compose.postgres.yml`, and `docker-compose.sqlite.yml`, and still publishes only the `pandar-web` runtime image.
- `frontend/auth/lib/utils.ts` no longer carries a duplicated implementation of the same `cn` helper; it re-exports `cn` from `../../lib/utils`.
- Auth and dashboard shadcn primitives remain app-local unless a primitive is proven equivalent and can be shared without changing its public contract.
- `README.md` and `docs/development.md` no longer instruct developers to run `npm --prefix frontend run build` for the main frontend build or `cd frontend/auth; npm install` as the primary auth-app setup flow.
- `docs/roadmap.md` records the completed frontend monorepo cleanup.

## Testing And Verification

Verification should cover both JavaScript and repository packaging paths:

- `npm install` at the repository root to regenerate the unified lockfile.
- `npm run test:web` from the root; expected exit code 0.
- `npm run test:auth` from the root; expected exit code 0.
- `npm run build:plugin-local` from the root; expected exit code 0 and checked-in `frontend/plugin-local/dist` remains current.
- `npm run build:web` from the root; expected exit code 0.
- `npm run build:auth` from the root; expected exit code 0.
- `nix build --show-trace .#checks.x86_64-linux.pandar-web`; expected exit code 0 when Nix has dependency access.
- `nix build --show-trace .#checks.x86_64-linux.pandar-auth`; expected exit code 0 when Nix has dependency access.
- `nix build --show-trace .#checks.x86_64-linux.pandar-auth-migrate`; expected exit code 0 when Nix has dependency access.
- `nix build --show-trace .#checks.x86_64-linux.pandar-auth-jwt-smoke`; expected exit code 0 when Nix has dependency access.
- `nix build --show-trace .#checks.x86_64-linux.pandar-auth-cookie-smoke`; expected exit code 0 when Nix has dependency access.
- `nix build --show-trace .#checks.x86_64-linux.pandar-web-auth-redirect-smoke`; expected exit code 0 when Nix has dependency access.
- `cargo fmt`; expected exit code 0.
- `cargo clippy`; expected exit code 0.
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`; expected exit code 0.
- `git status --short` before the final commit; expected output contains only files intentionally changed for this spec. After the final commit and push, `git status --short` should be empty.

If any required verification command is blocked by environment limits, missing network access for dependency hash discovery, or runtime duration, record the command, exit status, and the relevant output instead of treating it as passed.

## Out Of Scope

- Creating a shared frontend component package.
- Redesigning dashboard or auth UI primitives.
- Changing auth behavior, tenant onboarding behavior, or dashboard routes.
- Migrating from npm to pnpm, Yarn, or another package manager.
- Refactoring Rust code unrelated to frontend packaging.
