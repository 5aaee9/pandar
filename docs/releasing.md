# Releasing Pandar

This runbook prepares and publishes a Pandar release. The `v0.1.4` tag publishes desktop archives, Hub/Web container images, and the Helm chart. It does not publish an Android APK.

## 1. Prepare the release commit

Use a clean, up-to-date `main` checkout and verify that every declared product version matches the release:

```bash
git switch main
git pull --ff-only origin main
git status --short
bash scripts/check-release-version.sh 0.1.4
```

Review `CHANGELOG.md`, `docs/release-installation.md`, and the known limitations before tagging.

## 2. Run local release gates

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path Cargo.toml --workspace
cargo test --manifest-path tools/release-smoke/Cargo.toml

npm ci
npm audit --omit=dev --audit-level=moderate
npm run lint:web
npm run test:web
npm run test:auth
npm run typecheck:web
npm run typecheck:auth
npm run build:web
npm run build:auth

helm lint docs/deployment/kubernetes \
  --set hub.accessCodeEncryption.existingSecret=pandar-hub-secrets
scripts/check-deployment-security.sh
git diff --check
git status --short
```

The release commit must be pushed to `main`, and its GitHub `Checks` workflow must pass before tagging.

## 3. Create the release tag

Only after the release commit and remote Checks are green:

```bash
git switch main
git pull --ff-only origin main
test -z "$(git status --porcelain)"
bash scripts/check-release-version.sh 0.1.4
git tag -a v0.1.4 -m "Pandar 0.1.4"
git push origin v0.1.4
```

Pushing the tag starts:

- `Checks`, which repeats the quality gates and validates tag/version consistency;
- `Release`, which builds and target-architecture smoke-tests the Linux amd64, macOS amd64/arm64, and Windows amd64 three-file archives, and builds the Windows Studio hook bundle, before creating the GitHub Release; both macOS rows use Apple Silicon, with the amd64 checks running under Rosetta 2;
- `Docker`, which publishes `hub:v0.1.4`, `web:v0.1.4`, and Helm chart `0.1.4` after Checks succeeds.

Do not create the GitHub Release manually while these workflows are running.

## 4. Verify publication

```bash
gh run list --commit "$(git rev-list -n 1 v0.1.4)" --limit 20
gh release view v0.1.4
gh release download v0.1.4 --dir /tmp/pandar-v0.1.4
helm show chart oci://ghcr.io/projectpandar/pandar/chart/pandar --version 0.1.4
docker pull ghcr.io/projectpandar/pandar/hub:v0.1.4
docker pull ghcr.io/projectpandar/pandar/web:v0.1.4
```

The `v0.1.4` release is expected to have 60 desktop files: each of the six ABI series has four
`.tar.gz` archives and their `.sha256` sidecars, plus a Windows Studio hook `.zip` and its
`.sha256` sidecar. Run the checksum, CLI startup, and plugin checks from
`docs/release-installation.md` on every target host.

Record tagged-artifact evidence in `docs/compatibility/release-artifacts.md`, record real Studio evidence separately in `docs/compatibility/bambu-studio-plugin.md`, and update `docs/roadmap.md`. A failed or partial workflow is not a completed release.
