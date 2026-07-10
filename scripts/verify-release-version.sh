#!/usr/bin/env bash
# Verify version files match the semver parsed from a release tag.
set -euo pipefail

TAG="${1:?usage: verify-release-version.sh <tag>}"
if [[ ! "${TAG#v}" =~ ^([0-9]+\.[0-9]+\.[0-9]+) ]]; then
  echo "Could not derive semantic version from tag: ${TAG}"
  exit 1
fi
VERSION="${BASH_REMATCH[1]}"

check_file() {
  local file="$1"
  local actual="$2"
  if [ "$actual" != "$VERSION" ]; then
    echo "Version mismatch in $file: expected $VERSION, got $actual"
    exit 1
  fi
}

pkg_ver="$(node -p "require('./package.json').version")"
tauri_ver="$(node -p "JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json','utf8')).version")"
cargo_ver="$(node -e '
const fs = require("fs");
const lines = fs.readFileSync("src-tauri/Cargo.toml", "utf8").split("\n");
let inPackage = false;
for (const line of lines) {
  if (line.trim() === "[package]") { inPackage = true; continue; }
  if (inPackage && /^\[/.test(line.trim())) break;
  if (inPackage && line.startsWith("version")) {
    const match = line.match(/version\s*=\s*"([^"]+)"/);
    if (match) { console.log(match[1]); process.exit(0); }
  }
}
process.exit(1);
')"

check_file package.json "$pkg_ver"
check_file src-tauri/tauri.conf.json "$tauri_ver"
check_file src-tauri/Cargo.toml "$cargo_ver"

echo "Version files match tag ${TAG} (${VERSION})"
