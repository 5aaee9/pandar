#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
chart="$root/docs/deployment/kubernetes"
rendered="$(mktemp)"
trap 'rm -f "$rendered"' EXIT

helm template pandar "$chart" \
  --set hub.accessCodeEncryption.existingSecret=pandar-hub-secrets \
  --set hub.grpcTls.existingSecret=pandar-hub-grpc-tls >"$rendered"

for ingress in hub web; do
  if helm template pandar "$chart" \
    --set hub.accessCodeEncryption.existingSecret=pandar-hub-secrets \
    --set hub.grpcTls.existingSecret=pandar-hub-grpc-tls \
    --set "ingress.${ingress}.enabled=true" >/dev/null 2>&1; then
    echo "expected enabled $ingress ingress without TLS to fail" >&2
    exit 1
  fi
done

require_count() {
  local expected="$1"
  local pattern="$2"
  local actual
  actual="$(grep -Ec -- "$pattern" "$rendered" || true)"
  if [[ "$actual" != "$expected" ]]; then
    echo "expected $expected rendered matches for $pattern, found $actual" >&2
    exit 1
  fi
}

require_count 3 '^automountServiceAccountToken: false$|^      automountServiceAccountToken: false$'
require_count 2 '^        runAsNonRoot: true$'
require_count 1 '^        runAsUser: 10001$'
require_count 1 '^        runAsUser: 1000$'
require_count 2 '^            allowPrivilegeEscalation: false$'
require_count 2 '^            readOnlyRootFilesystem: true$'
require_count 2 '^              - ALL$'
require_count 2 '^          type: RuntimeDefault$'
require_count 1 '^              mountPath: /app/frontend/.next/cache$'
require_count 2 '^              mountPath: /tmp$'

grep -Fx 'USER 10001:10001' "$root/Dockerfile" >/dev/null
grep -Fx 'USER 1000:1000' "$root/frontend/Dockerfile" >/dev/null
[[ "$(grep -c '^FROM .*@sha256:' "$root/Dockerfile")" == 2 ]]
[[ "$(grep -c '^FROM .*@sha256:' "$root/frontend/Dockerfile")" == 3 ]]
grep -Fx 'appVersion: "0.1.1"' "$chart/Chart.yaml" >/dev/null
