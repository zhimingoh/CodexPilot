#!/usr/bin/env bash
set -euo pipefail

runtime_files=(
  "crates/codex-pilot-core/src"
  "apps/codex-pilot-manager/src-tauri/src"
)

violations=""

while IFS= read -r line; do
  file="${line%%:*}"
  rest="${line#*:}"
  lineno="${rest%%:*}"
  code="${rest#*:}"

  if [[ "$file" == "crates/codex-pilot-core/src/windows_integration.rs" ]]; then
    continue
  fi

  if [[ "$code" == *"windows_integration::std_command"* ]] || [[ "$code" == *"windows_integration::tokio_command"* ]]; then
    continue
  fi

  violations+="${file}:${lineno}: ${code}"$'\n'
done < <(rg -n "Command::new\(" "${runtime_files[@]}")

if [[ -n "$violations" ]]; then
  echo "Windows subprocess hygiene violations found:"
  printf '%s' "$violations"
  exit 1
fi
