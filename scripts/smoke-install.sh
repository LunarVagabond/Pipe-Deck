#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> Rust unit tests (includes restore/config compatibility)"
export PIPE_DECK_USE_MOCK=1
make test

echo "==> Frontend type-check"
make check

echo "==> Daemon binary compiles"
make build-daemon-dev

echo "==> CLI smoke (mock graph)"
export PIPE_DECK_BUNDLED_PLUGINS="$ROOT/plugins"
export PIPE_DECK_USE_MOCK=1
make build-cli
"$CLI_BIN_DEBUG" plugins list >/dev/null
"$CLI_BIN_DEBUG" graph >/dev/null

if [[ -f src-tauri/target/release/bundle/deb/*.deb ]]; then
  echo "==> Debian package smoke install"
  DEB="$(ls -1 src-tauri/target/release/bundle/deb/*.deb | head -n1)"
  sudo dpkg -i "$DEB" || sudo apt-get install -f -y
  command -v pipe-deck
  command -v pipe-deck-daemon || echo "warn: pipe-deck-daemon not in package path yet"
else
  echo "==> No .deb artifact present; skipping install smoke test"
fi

echo "Smoke checks passed."
