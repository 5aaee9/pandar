#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
migration_root="$repo_root/crates/pandar-hub/migrations"
shared="$migration_root/shared"
overrides="$migration_root/overrides"
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT

for backend in sqlite postgres; do
  output="$temporary/$backend"
  mkdir -p "$output"
  cp "$shared"/*.sql "$output/"
  cp "$overrides/$backend"/*.sql "$output/"
done

for backend in sqlite postgres; do
  other=postgres
  if [[ "$backend" == postgres ]]; then
    other=sqlite
  fi
  for override in "$overrides/$backend"/*.sql; do
    name=${override##*/}
    if [[ -e "$shared/$name" ]]; then
      echo "migration $name cannot be both shared and backend-specific" >&2
      exit 1
    fi
    if [[ ! -e "$overrides/$other/$name" ]]; then
      echo "migration $name has a $backend override but no $other override" >&2
      exit 1
    fi
  done
done

if [[ ${1:-} == --check ]]; then
  for backend in sqlite postgres; do
    if ! diff -ru "$migration_root/$backend" "$temporary/$backend"; then
      echo "generated $backend migrations are stale; run scripts/sync-hub-migrations.sh" >&2
      exit 1
    fi
  done
  exit 0
fi

if [[ $# -ne 0 ]]; then
  echo "usage: scripts/sync-hub-migrations.sh [--check]" >&2
  exit 2
fi

for backend in sqlite postgres; do
  rm -rf "$migration_root/$backend"
  cp -R "$temporary/$backend" "$migration_root/$backend"
done
