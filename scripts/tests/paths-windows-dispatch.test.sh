#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEST_ROOT"' EXIT

# Select the production Windows cfg body while keeping the host standard library.
if ! rustc --edition=2021 --test --cfg 'target_os="windows"' \
  -Aexplicit_builtin_cfgs_in_flags \
  "$REPO_ROOT/src-tauri/src/paths_windows_dispatch_tests.rs" \
  -o "$TEST_ROOT/paths-windows-dispatch-tests.exe"; then
  printf 'Windows path-dispatch harness compilation failed\n' >&2
  exit 1
fi

if ! "$TEST_ROOT/paths-windows-dispatch-tests.exe" \
  --exact windows_public_wrappers_use_appdata; then
  printf 'ASSERTION FAILED: Windows path dispatch must use APPDATA\n' >&2
  exit 1
fi
