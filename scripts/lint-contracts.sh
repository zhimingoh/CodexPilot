#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"

scripts/check-windows-hygiene.sh

runtime_dirs=(
  "crates"
  "apps/codex-pilot-manager/src-tauri/src"
  "apps/codex-pilot-manager/src"
)

logging_violations="$(rg -n "println!\(" "${runtime_dirs[@]}" -g '!apps/codex-pilot-manager/src-tauri/build.rs' || true)"

if [[ -n "$logging_violations" ]]; then
  echo "Runtime println! violations found:"
  printf '%s\n' "$logging_violations"
  exit 1
fi
