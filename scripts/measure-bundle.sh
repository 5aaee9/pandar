#!/usr/bin/env bash
set -euo pipefail

# Bundle size measurement script for Pandar frontend
# Measures initial JS and CSS sizes for dashboard routes

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FRONTEND_DIR="$REPO_ROOT/frontend"
BASELINE_FILE="$SCRIPT_DIR/bundle-baseline.json"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
  echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
  echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
  echo -e "${RED}[ERROR]${NC} $1"
}

# Check Node version
check_node_version() {
  local node_version
  node_version=$(node --version | cut -d'v' -f2 | cut -d'.' -f1)
  if [[ "$node_version" != "24" ]]; then
    log_error "Node 24 required, found v$node_version"
    exit 1
  fi
}

# Measure bundle sizes for a route
measure_route() {
  local route=$1
  local build_dir=$2
  
  # Try route group path first, fallback to direct path
  local manifest_file="$build_dir/server/app/(dashboard)/$route/page/build-manifest.json"
  if [[ ! -f "$manifest_file" ]]; then
    manifest_file="$build_dir/server/app/$route/page/build-manifest.json"
  fi
  if [[ ! -f "$manifest_file" ]]; then
    log_error "Build manifest not found for route: $route"
    return 1
  fi
  
  # Extract JS files from manifest
  local js_files
  js_files=$(jq -r '.rootMainFiles[]' "$manifest_file" 2>/dev/null || echo "")
  
  # Measure JS size (gzipped)
  local js_size=0
  for file in $js_files; do
    local file_path="$build_dir/$file"
    if [[ -f "$file_path" ]]; then
      local size
      size=$(gzip -9 -c "$file_path" | wc -c)
      js_size=$((js_size + size))
    else
      log_warn "Chunk file not found: $file_path"
    fi
  done
  
  # Measure CSS size (gzipped)
  local css_size=0
  if [[ -d "$build_dir/static/chunks" ]]; then
    for css_file in "$build_dir/static/chunks"/*.css; do
      if [[ -f "$css_file" ]]; then
        local size
        size=$(gzip -9 -c "$css_file" | wc -c)
        css_size=$((css_size + size))
      fi
    done
  fi
  
  echo "{\"js\": $js_size, \"css\": $css_size}"
}

# Generate baseline
generate_baseline() {
  local commit=$1
  
  log_info "Generating baseline for commit: $commit"
  
  # Create temporary worktree
  local worktree_dir
  worktree_dir=$(mktemp -d)
  
  log_info "Creating worktree at: $worktree_dir"
  git worktree add "$worktree_dir" "$commit" || {
    log_error "Failed to create worktree"
    exit 1
  }
  
  # Install dependencies and build
  log_info "Installing dependencies..."
  (cd "$worktree_dir/frontend" && npm install) || {
    log_error "Failed to install dependencies"
    git worktree remove "$worktree_dir" --force
    exit 1
  }
  
  log_info "Building..."
  (cd "$worktree_dir/frontend" && NODE_ENV=production npm run build) || {
    log_error "Failed to build"
    git worktree remove "$worktree_dir" --force
    exit 1
  }
  
  # Measure routes
  log_info "Measuring bundle sizes..."
  local settings_size devices_size jobs_size
  settings_size=$(measure_route "settings" "$worktree_dir/frontend/.next")
  devices_size=$(measure_route "devices" "$worktree_dir/frontend/.next")
  jobs_size=$(measure_route "jobs" "$worktree_dir/frontend/.next")
  
  # Write baseline
  cat > "$BASELINE_FILE" <<EOF
{
  "commit": "$commit",
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "settings": $settings_size,
  "devices": $devices_size,
  "jobs": $jobs_size
}
EOF
  
  log_info "Baseline written to: $BASELINE_FILE"
  cat "$BASELINE_FILE"
  
  # Cleanup
  log_info "Cleaning up worktree..."
  git worktree remove "$worktree_dir" --force
  
  log_info "Baseline generation complete"
}

# Compare against baseline
compare() {
  if [[ ! -f "$BASELINE_FILE" ]]; then
    log_error "Baseline file not found: $BASELINE_FILE"
    log_error "Run: $0 --generate-baseline <commit>"
    exit 1
  fi
  
  log_info "Building current branch..."
  (cd "$FRONTEND_DIR" && NODE_ENV=production npm run build) || {
    log_error "Failed to build"
    exit 1
  }
  
  # Measure current sizes
  log_info "Measuring current bundle sizes..."
  local settings_size devices_size jobs_size
  settings_size=$(measure_route "settings" "$FRONTEND_DIR/.next")
  devices_size=$(measure_route "devices" "$FRONTEND_DIR/.next")
  jobs_size=$(measure_route "jobs" "$FRONTEND_DIR/.next")
  
  # Read baseline
  local baseline_commit baseline_settings baseline_devices baseline_jobs
  baseline_commit=$(jq -r '.commit' "$BASELINE_FILE")
  baseline_settings=$(jq -r '.settings.js' "$BASELINE_FILE")
  baseline_devices=$(jq -r '.devices.js' "$BASELINE_FILE")
  baseline_jobs=$(jq -r '.jobs.js' "$BASELINE_FILE")
  
  local baseline_settings_css baseline_devices_css baseline_jobs_css
  baseline_settings_css=$(jq -r '.settings.css' "$BASELINE_FILE")
  baseline_devices_css=$(jq -r '.devices.css' "$BASELINE_FILE")
  baseline_jobs_css=$(jq -r '.jobs.css' "$BASELINE_FILE")
  
  # Extract current sizes
  local current_settings_js current_devices_js current_jobs_js
  current_settings_js=$(echo "$settings_size" | jq -r '.js')
  current_devices_js=$(echo "$devices_size" | jq -r '.js')
  current_jobs_js=$(echo "$jobs_size" | jq -r '.js')
  
  local current_settings_css current_devices_css current_jobs_css
  current_settings_css=$(echo "$settings_size" | jq -r '.css')
  current_devices_css=$(echo "$devices_size" | jq -r '.css')
  current_jobs_css=$(echo "$jobs_size" | jq -r '.css')
  
  # Calculate reductions
  local settings_js_reduction devices_js_reduction jobs_js_reduction
  settings_js_reduction=$((baseline_settings - current_settings_js))
  devices_js_reduction=$((baseline_devices - current_devices_js))
  jobs_js_reduction=$((baseline_jobs - current_jobs_js))
  
  local settings_js_percent devices_js_percent jobs_js_percent
  settings_js_percent=$(echo "scale=2; $settings_js_reduction * 100 / $baseline_settings" | bc)
  devices_js_percent=$(echo "scale=2; $devices_js_reduction * 100 / $baseline_devices" | bc)
  jobs_js_percent=$(echo "scale=2; $jobs_js_reduction * 100 / $baseline_jobs" | bc)
  
  # Check CSS constraint
  local css_violation=0
  if [[ $current_settings_css -gt $baseline_settings_css ]]; then
    log_error "CSS size increased for /settings: $baseline_settings_css -> $current_settings_css"
    css_violation=1
  fi
  if [[ $current_devices_css -gt $baseline_devices_css ]]; then
    log_error "CSS size increased for /devices: $baseline_devices_css -> $current_devices_css"
    css_violation=1
  fi
  if [[ $current_jobs_css -gt $baseline_jobs_css ]]; then
    log_error "CSS size increased for /jobs: $baseline_jobs_css -> $current_jobs_css"
    css_violation=1
  fi
  
  # Output results
  echo ""
  log_info "Bundle Size Comparison (baseline: $baseline_commit)"
  echo ""
  printf "%-12s %15s %15s %15s %15s\n" "Route" "Baseline JS" "Current JS" "Reduction" "Percent"
  printf "%-12s %15s %15s %15s %15s\n" "-----" "-----------" "----------" "---------" "-------"
  printf "%-12s %15d %15d %15d %14s%%\n" "settings" "$baseline_settings" "$current_settings_js" "$settings_js_reduction" "$settings_js_percent"
  printf "%-12s %15d %15d %15d %14s%%\n" "devices" "$baseline_devices" "$current_devices_js" "$devices_js_reduction" "$devices_js_percent"
  printf "%-12s %15d %15d %15d %14s%%\n" "jobs" "$baseline_jobs" "$current_jobs_js" "$jobs_js_reduction" "$jobs_js_percent"
  echo ""
  printf "%-12s %15s %15s\n" "Route" "Baseline CSS" "Current CSS"
  printf "%-12s %15s %15s\n" "-----" "------------" "-----------"
  printf "%-12s %15d %15d\n" "settings" "$baseline_settings_css" "$current_settings_css"
  printf "%-12s %15d %15d\n" "devices" "$baseline_devices_css" "$current_devices_css"
  printf "%-12s %15d %15d\n" "jobs" "$baseline_jobs_css" "$current_jobs_css"
  echo ""
  
  if [[ $css_violation -eq 1 ]]; then
    log_error "CSS constraint violated"
    exit 1
  fi
  
  log_info "Comparison complete"
}

# Main
check_node_version

case "${1:-}" in
  --generate-baseline)
    if [[ -z "${2:-}" ]]; then
      log_error "Usage: $0 --generate-baseline <commit>"
      exit 1
    fi
    generate_baseline "$2"
    ;;
  --compare)
    compare
    ;;
  *)
    log_error "Usage: $0 [--generate-baseline <commit>|--compare]"
    exit 1
    ;;
esac
