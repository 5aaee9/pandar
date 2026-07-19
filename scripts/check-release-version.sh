#!/usr/bin/env bash
set -euo pipefail

expected="${1:?usage: bash scripts/check-release-version.sh <version>}"

if [[ ! "$expected" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "invalid release version: $expected" >&2
  exit 1
fi

check_version() {
  local path="$1"
  local actual="$2"

  if [[ "$actual" != "$expected" ]]; then
    echo "$path declares version '$actual', expected '$expected'" >&2
    exit 1
  fi
}

for manifest in crates/*/Cargo.toml tools/*/Cargo.toml; do
  version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$manifest" | head -n 1)"
  check_version "$manifest" "$version"
done

while IFS= read -r version; do
  check_version "nix/pandar.nix" "$version"
done < <(sed -n 's/^[[:space:]]*version = "\([^"]*\)";/\1/p' nix/pandar.nix)

nix_agent_version="$(sed -n 's/.*runCommand "pandar-agent-\([^"]*\)".*/\1/p' nix/pandar.nix | head -n 1)"
check_version "nix/pandar.nix pandar-agent derivation" "$nix_agent_version"

node - "$expected" <<'JS'
const fs = require("node:fs");

const expected = process.argv[2];
for (const path of ["package.json", "frontend/package.json", "frontend/auth/package.json"]) {
  const actual = JSON.parse(fs.readFileSync(path)).version;
  if (actual !== expected) {
    throw new Error(`${path} declares version '${actual}', expected '${expected}'`);
  }
}

const lock = JSON.parse(fs.readFileSync("package-lock.json"));
for (const packagePath of ["", "frontend", "frontend/auth"]) {
  const actual = lock.packages[packagePath].version;
  if (actual !== expected) {
    const label = packagePath || "<root>";
    throw new Error(
      `package-lock.json package ${label} declares version '${actual}', expected '${expected}'`,
    );
  }
}
JS

android_version="$(sed -n 's/^[[:space:]]*versionName = "\([^"]*\)"/\1/p' mobile/android/app/build.gradle.kts | head -n 1)"
check_version "mobile/android/app/build.gradle.kts" "$android_version"

chart_version="$(sed -n 's/^version:[[:space:]]*//p' docs/deployment/kubernetes/Chart.yaml | tr -d '"' | head -n 1)"
check_version "docs/deployment/kubernetes/Chart.yaml" "$chart_version"

if ! grep -Fq "## [$expected]" CHANGELOG.md; then
  echo "CHANGELOG.md has no $expected release section" >&2
  exit 1
fi

echo "all release versions match $expected"
