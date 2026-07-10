#!/usr/bin/env bash
# Stage Linux release assets and generate latest.json for GitHub Releases.
set -euo pipefail

TAG="${1:?usage: stage-release.sh <tag> <version>}"
VERSION="${2:?usage: stage-release.sh <tag> <version>}"
REPO="${GITHUB_REPOSITORY:-LunarVagabond/Pipe-Deck}"
OUT_DIR="${3:-release-files}"
BUNDLE="src-tauri/target/release/bundle"
RELEASE_BASE="https://github.com/${REPO}/releases/download/${TAG}"

mkdir -p "$OUT_DIR"
shopt -s nullglob

apps=("$BUNDLE"/appimage/*.AppImage)
debs=("$BUNDLE"/deb/*.deb)
rpms=("$BUNDLE"/rpm/*.rpm)

if [ ${#apps[@]} -eq 0 ]; then
  echo "No AppImage found under $BUNDLE/appimage"
  exit 1
fi
if [ ${#debs[@]} -eq 0 ]; then
  echo "No .deb found under $BUNDLE/deb"
  exit 1
fi
if [ ${#rpms[@]} -eq 0 ]; then
  echo "No .rpm found under $BUNDLE/rpm"
  exit 1
fi

app_src="${apps[0]}"
deb_src="${debs[0]}"
rpm_src="${rpms[0]}"

app_out="pipe-deck-${TAG}-linux-x86_64.AppImage"
deb_out="pipe-deck_${VERSION}_amd64.deb"
rpm_out="pipe-deck-${VERSION}.x86_64.rpm"
binary_out="pipe-deck-${TAG}-linux-x86_64.tar.gz"

cp -f "$app_src" "$OUT_DIR/$app_out"
cp -f "$deb_src" "$OUT_DIR/$deb_out"
cp -f "$rpm_src" "$OUT_DIR/$rpm_out"

app_sig_src="${app_src}.sig"
if [ ! -f "$app_sig_src" ]; then
  echo "Missing AppImage signature: $app_sig_src"
  exit 1
fi
cp -f "$app_sig_src" "$OUT_DIR/${app_out}.sig"
signature="$(tr -d '\r\n' < "$OUT_DIR/${app_out}.sig")"

gui_bin="src-tauri/target/release/pipe-deck"
daemon_bin="src-tauri/target/release/pipe-deck-daemon"
if [ ! -f "$gui_bin" ] || [ ! -f "$daemon_bin" ]; then
  echo "Missing release binaries: $gui_bin or $daemon_bin"
  exit 1
fi

tar -czf "$OUT_DIR/$binary_out" -C "src-tauri/target/release" pipe-deck pipe-deck-daemon

pub_date="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

node -e "
const fs = require('fs');
const manifest = {
  version: process.argv[1],
  notes: '',
  pub_date: process.argv[2],
  platforms: {
    'linux-x86_64-appimage': {
      signature: process.argv[3],
      url: process.argv[4],
    },
    'linux-x86_64-deb': { url: process.argv[5] },
    'linux-x86_64-rpm': { url: process.argv[6] },
    'linux-x86_64-binary': { url: process.argv[7] },
  },
};
fs.writeFileSync(process.argv[8], JSON.stringify(manifest, null, 2) + '\n');
" \
  "$VERSION" \
  "$pub_date" \
  "$signature" \
  "${RELEASE_BASE}/${app_out}" \
  "${RELEASE_BASE}/${deb_out}" \
  "${RELEASE_BASE}/${rpm_out}" \
  "${RELEASE_BASE}/${binary_out}" \
  "$OUT_DIR/latest.json"

echo "Staged release files:"
ls -la "$OUT_DIR"
