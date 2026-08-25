#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$REPO_ROOT/scripts/fix-appimage-glib.sh"
TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEST_ROOT"' EXIT

fail() {
  echo "ASSERTION FAILED: $*" >&2
  exit 1
}

make_fake_tools() {
  local bin_dir="$1"
  mkdir -p "$bin_dir"

  cat > "$bin_dir/unsquashfs" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[ "$1" = "-d" ] || exit 64
mkdir -p "$2"
cp -a "$FIXTURE_APPDIR/." "$2/"
echo unsquashfs >> "$EVENT_LOG"
EOF

  cat > "$bin_dir/mksquashfs" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
mkdir -p "$CAPTURED_APPDIR"
cp -a "$1/." "$CAPTURED_APPDIR/"
printf 'fake-squashfs\n' > "$2"
echo mksquashfs >> "$EVENT_LOG"
EOF

  chmod +x "$bin_dir/unsquashfs" "$bin_dir/mksquashfs"
}

make_fake_appimage() {
  local appimage="$1"
  cat > "$appimage" <<'EOF'
#!/usr/bin/env bash
if [ "${1:-}" = "--appimage-offset" ]; then
  printf '0\n'
  exit 0
fi
exit 64
EOF
  chmod +x "$appimage"
}

new_case() {
  local name="$1"
  CASE_ROOT="$TEST_ROOT/$name"
  FIXTURE_APPDIR="$CASE_ROOT/fixture"
  CAPTURED_APPDIR="$CASE_ROOT/captured"
  EVENT_LOG="$CASE_ROOT/events.log"
  FAKE_BIN="$CASE_ROOT/bin"
  APPIMAGE="$CASE_ROOT/test.AppImage"
  mkdir -p "$FIXTURE_APPDIR/usr/lib"
  : > "$EVENT_LOG"
  make_fake_tools "$FAKE_BIN"
  make_fake_appimage "$APPIMAGE"
  export FIXTURE_APPDIR CAPTURED_APPDIR EVENT_LOG
}

run_post_processor() {
  PATH="$FAKE_BIN:$PATH" bash "$SCRIPT" "$APPIMAGE"
}

assert_no_banned_libs() {
  local root="$1"
  local residue
  residue="$(find "$root" \( -type f -o -type l \) \
    \( -name 'libglib-2.0.so*' -o -name 'libgobject-2.0.so*' \
       -o -name 'libgio-2.0.so*' -o -name 'libgmodule-2.0.so*' \
       -o -name 'libpipewire*.so*' -o -name 'libspa*.so*' \) \
    -printf '%P\n' | LC_ALL=C sort)"
  [ -z "$residue" ] || fail "banned libraries remain after AppImage post-processing: $residue"
}

new_case strips_files_and_symlinks
mkdir -p "$FIXTURE_APPDIR/usr/lib/spa-0.2/support"
touch "$FIXTURE_APPDIR/usr/lib/libglib-2.0.so.0"
touch "$FIXTURE_APPDIR/usr/lib/libpipewire-0.3.so.0.1300.1"
ln -s libpipewire-0.3.so.0.1300.1 "$FIXTURE_APPDIR/usr/lib/libpipewire-0.3.so.0"
touch "$FIXTURE_APPDIR/usr/lib/spa-0.2/support/libspa-support.so.0"
touch "$FIXTURE_APPDIR/usr/lib/libkeep.so.1"
run_post_processor
assert_no_banned_libs "$CAPTURED_APPDIR"
[ -f "$CAPTURED_APPDIR/usr/lib/libkeep.so.1" ] || fail "unrelated library was removed"
[ "$(printf '%s\n' "$(<"$EVENT_LOG")")" = $'unsquashfs\nmksquashfs' ] || \
  fail "AppImage extraction/repack order was not preserved"

new_case allows_missing_spa
touch "$FIXTURE_APPDIR/usr/lib/libglib-2.0.so.0"
touch "$FIXTURE_APPDIR/usr/lib/libpipewire-0.3.so.0"
run_post_processor
assert_no_banned_libs "$CAPTURED_APPDIR"

new_case rejects_missing_glib_independently
touch "$FIXTURE_APPDIR/usr/lib/libpipewire-0.3.so.0"
if output="$(run_post_processor 2>&1)"; then
  fail "missing GLib family was accepted because another family was removed"
fi
case "$output" in
  *"No bundled GLib libs found"*) ;;
  *) fail "missing GLib family did not produce its family-specific error: $output" ;;
esac

new_case rejects_missing_pipewire_independently
touch "$FIXTURE_APPDIR/usr/lib/libglib-2.0.so.0"
if output="$(run_post_processor 2>&1)"; then
  fail "missing PipeWire family was accepted because another family was removed"
fi
case "$output" in
  *"No bundled PipeWire libs found"*) ;;
  *) fail "missing PipeWire family did not produce its family-specific error: $output" ;;
esac

make_dry_run="$(make -C "$REPO_ROOT" -n build NPM=fake-npm CARGO=fake-cargo)"
case "$make_dry_run" in
  *'bash scripts/fix-appimage-glib.sh "$appimage";'*'exec tauri signer sign "$appimage";'*) ;;
  *) fail "Makefile no longer post-processes the AppImage before updater signing" ;;
esac

echo "PASS: AppImage post-processing removes each banned library family"
