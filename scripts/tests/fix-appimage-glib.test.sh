#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$REPO_ROOT/scripts/fix-appimage-glib.sh"
SYSTEM_FIND="$(command -v find)"
TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEST_ROOT"' EXIT

fail() {
  echo "ASSERTION FAILED: $*" >&2
  exit 1
}

arrays_equal() {
  local -n left_array="$1"
  local -n right_array="$2"
  [ "${#left_array[@]}" -eq "${#right_array[@]}" ] || return 1
  local index
  for ((index = 0; index < ${#left_array[@]}; index++)); do
    [ "${left_array[$index]}" = "${right_array[$index]}" ] || return 1
  done
}

make_fake_tools() {
  local bin_dir="$1"
  mkdir -p "$bin_dir"

  cat > "$bin_dir/unsquashfs" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[ "$1" = "-d" ] || exit 64
source_path="${3:-}"
if source_digest="$(sha256sum -- "$source_path" 2>/dev/null)"; then
  source_digest="${source_digest%% *}"
else
  source_digest=""
fi
if [ "${source_path##*/}" != fs.squashfs ] || \
   [ "$source_digest" != "$EXPECTED_SQUASHFS_SHA256" ] || \
   ! cmp -s -- "$source_path" "$EXPECTED_SQUASHFS_FILE"; then
  printf 'extractor received wrong AppImage payload\n' >&2
  exit 68
fi
if [ "${FAKE_UNSQUASHFS_STATUS:-0}" -ne 0 ]; then
  printf 'unsquashfs-fail\n' >> "$EVENT_LOG"
  exit "$FAKE_UNSQUASHFS_STATUS"
fi
mkdir -p "$2"
cp -a -- "$FIXTURE_APPDIR/." "$2/"
printf 'unsquashfs\n' >> "$EVENT_LOG"
EOF

  cat > "$bin_dir/mksquashfs" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "${FAKE_MKSQUASHFS_STATUS:-0}" -ne 0 ]; then
  printf 'mksquashfs-fail\n' >> "$EVENT_LOG"
  exit "$FAKE_MKSQUASHFS_STATUS"
fi
mkdir -p "$CAPTURED_APPDIR"
cp -a -- "$1/." "$CAPTURED_APPDIR/"
printf 'FINAL-SQUASHFS-BYTES\n' > "$2"
printf 'mksquashfs\n' >> "$EVENT_LOG"
EOF

  cat > "$bin_dir/fake-cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'cargo\n' >> "$EVENT_LOG"
mkdir -p "$CARGO_TARGET_DIR/release"
printf 'fake-daemon\n' > "$CARGO_TARGET_DIR/release/pipe-deck-daemon"
EOF

  cat > "$bin_dir/fake-npm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-} ${2:-} ${3:-}" in
  "run tauri build")
    printf 'npm-build\n' >> "$EVENT_LOG"
    ;;
  "exec tauri signer")
    [ "${4:-}" = "sign" ] || exit 65
    [ -n "${5:-}" ] || exit 66
    printf 'npm-sign\n' >> "$EVENT_LOG"
    cp -- "$5" "$SIGNED_APPIMAGE"
    printf 'fake-signature\n' > "$5.sig"
    ;;
  *)
    exit 67
    ;;
esac
EOF

  cat > "$bin_dir/find" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
args=("$@")
printf '%s\0' "${#args[@]}" "${args[@]}" >> "$FIND_LOG"
exec "$SYSTEM_FIND" "${args[@]}"
EOF

  chmod +x "$bin_dir/unsquashfs" "$bin_dir/mksquashfs" \
    "$bin_dir/fake-cargo" "$bin_dir/fake-npm" "$bin_dir/find"
}

make_fake_appimage() {
  local appimage="$1"
  mkdir -p "$(dirname "$appimage")"
  cat > "$appimage" <<'EOF'
#!/usr/bin/env bash
if [ "${1:-}" = "--appimage-offset" ]; then
  printf '%s\n' "${FAKE_APPIMAGE_OFFSET:?}"
  exit 0
fi
exit 64
EOF
  printf '%0500s' '' >> "$appimage"
  printf 'FAKE-SQUASHFS-PAYLOAD\n' >> "$appimage"
  chmod +x "$appimage"
}

new_case() {
  local name="$1"
  local special_path="${2:-false}"
  if [ "$special_path" = true ]; then
    CASE_ROOT="$TEST_ROOT/$name path with spaces [*]?"
  else
    CASE_ROOT="$TEST_ROOT/$name"
  fi
  FIXTURE_APPDIR="$CASE_ROOT/fixture AppDir [*]?"
  CAPTURED_APPDIR="$CASE_ROOT/captured AppDir [*]?"
  EVENT_LOG="$CASE_ROOT/events.log"
  FIND_LOG="$CASE_ROOT/find-arguments.log"
  FAKE_BIN="$CASE_ROOT/bin"
  APPIMAGE="$CASE_ROOT/test image [*]?.AppImage"
  mkdir -p "$FIXTURE_APPDIR/usr/lib"
  : > "$EVENT_LOG"
  : > "$FIND_LOG"
  make_fake_tools "$FAKE_BIN"
  FAKE_APPIMAGE_OFFSET=512
  export FAKE_APPIMAGE_OFFSET
  make_fake_appimage "$APPIMAGE"
  EXPECTED_SQUASHFS_FILE="$CASE_ROOT/expected fs.squashfs [*]?"
  dd if="$APPIMAGE" of="$EXPECTED_SQUASHFS_FILE" bs=1 \
    skip="$FAKE_APPIMAGE_OFFSET" status=none
  EXPECTED_SQUASHFS_SHA256="$(sha256sum -- "$EXPECTED_SQUASHFS_FILE")"
  EXPECTED_SQUASHFS_SHA256="${EXPECTED_SQUASHFS_SHA256%% *}"
  FAKE_UNSQUASHFS_STATUS=0
  FAKE_MKSQUASHFS_STATUS=0
  SIGNED_APPIMAGE="$CASE_ROOT/signer-input.AppImage"
  export FIXTURE_APPDIR CAPTURED_APPDIR EVENT_LOG FIND_LOG SYSTEM_FIND
  export EXPECTED_SQUASHFS_FILE EXPECTED_SQUASHFS_SHA256
  export FAKE_UNSQUASHFS_STATUS FAKE_MKSQUASHFS_STATUS SIGNED_APPIMAGE
}

run_post_processor() {
  local caller_dir="${RUN_POST_PROCESSOR_CWD:-$PWD}"
  (cd "$caller_dir" && PATH="$FAKE_BIN:$PATH" bash "$SCRIPT" "$APPIMAGE")
}

run_post_processor_successfully() {
  local label="$1"
  local output status
  if output="$(run_post_processor 2>&1)"; then
    return 0
  else
    status=$?
  fi
  fail "$label returned status $status: $output"
}

assert_no_banned_libs() {
  local root="$1"
  local residue
  residue="$(find "$root" \( -type f -o -type l \) \
    \( -name 'libglib-2.0.so*' -o -name 'libgobject-2.0.so*' \
       -o -name 'libgio-2.0.so*' -o -name 'libgmodule-2.0.so*' \
       -o -name 'libpipewire*.so*' -o -name 'libspa-*.so*' \) \
    -printf '%P\n' | LC_ALL=C sort)"
  [ -z "$residue" ] || fail "banned libraries remain after AppImage post-processing: $residue"
}

case_extractor_authenticates_sliced_payload() {
  new_case extractor_authenticates_sliced_payload true
  touch "$FIXTURE_APPDIR/usr/lib/libglib-2.0.so.0"
  touch "$FIXTURE_APPDIR/usr/lib/libpipewire-0.3.so.0"
  run_post_processor_successfully "authenticated extractor payload fixture"
  assert_no_banned_libs "$CAPTURED_APPDIR"
}

case_spa_near_prefix_controls_survive() {
  new_case spa_near_prefix_controls_survive true
  mkdir -p "$FIXTURE_APPDIR/usr/lib/spa controls [*]?"
  touch "$FIXTURE_APPDIR/usr/lib/libglib-2.0.so.0"
  printf 'intended-spa-library\n' > \
    "$FIXTURE_APPDIR/usr/lib/spa controls [*]?/libspa-support.so.0"
  printf 'spandsp-control-bytes\n' > \
    "$FIXTURE_APPDIR/usr/lib/spa controls [*]?/libspandsp.so.2"
  printf 'spatialite-control-bytes\n' > \
    "$FIXTURE_APPDIR/usr/lib/spa controls [*]?/libspatialite.so.7"

  run_post_processor_successfully "SPA near-prefix control fixture"
  assert_no_banned_libs "$CAPTURED_APPDIR"
  cmp -s -- \
    "$FIXTURE_APPDIR/usr/lib/spa controls [*]?/libspandsp.so.2" \
    "$CAPTURED_APPDIR/usr/lib/spa controls [*]?/libspandsp.so.2" || \
    fail "unrelated near-prefix shared library was removed or changed: libspandsp.so.2"
  cmp -s -- \
    "$FIXTURE_APPDIR/usr/lib/spa controls [*]?/libspatialite.so.7" \
    "$CAPTURED_APPDIR/usr/lib/spa controls [*]?/libspatialite.so.7" || \
    fail "unrelated near-prefix shared library was removed or changed: libspatialite.so.7"
}

case_spa_selector_contract() {
  new_case spa_selector_contract
  touch "$FIXTURE_APPDIR/usr/lib/libglib-2.0.so.0"
  touch "$FIXTURE_APPDIR/usr/lib/libspa-support.so.0"
  printf 'case-sensitive control\n' > "$FIXTURE_APPDIR/usr/lib/LIBSPA-support.so.0"

  local RUN_POST_PROCESSOR_CWD="$CASE_ROOT/caller glob source"
  mkdir -p "$RUN_POST_PROCESSOR_CWD"
  touch "$RUN_POST_PROCESSOR_CWD/libspa-expanded.so.0"

  run_post_processor_successfully "SPA selector contract fixture"

  local -a find_tokens=()
  mapfile -d '' -t find_tokens < "$FIND_LOG"
  local token_index=0
  local invocation_count=0
  local argument_count
  local exact_spa_removal_count=0
  local exact_spa_residue_count=0
  local spa_removal_root=""
  local spa_residue_root=""
  local -a invocation_args=()
  local -a expected_spa_removal=()
  local -a expected_spa_residue=()
  while [ "$token_index" -lt "${#find_tokens[@]}" ]; do
    argument_count="${find_tokens[$token_index]}"
    token_index=$((token_index + 1))
    case "$argument_count" in
      ''|*[!0-9]*) fail "find argument transcript has an invalid argument count: $argument_count" ;;
    esac
    [ "$((token_index + argument_count))" -le "${#find_tokens[@]}" ] || \
      fail "find argument transcript ended inside invocation $((invocation_count + 1))"
    invocation_count=$((invocation_count + 1))
    invocation_args=("${find_tokens[@]:$token_index:$argument_count}")
    local invocation_root="${invocation_args[0]:-}"
    expected_spa_removal=(
      "$invocation_root" '(' -type f -o -type l ')' '(' -name 'libspa-*.so*' ')' -print0
    )
    expected_spa_residue=(
      "$invocation_root" '(' -type f -o -type l ')' '(' -name 'libspa-*.so*' ')' -print -quit
    )
    if arrays_equal invocation_args expected_spa_removal; then
      exact_spa_removal_count=$((exact_spa_removal_count + 1))
      spa_removal_root="$invocation_root"
    elif arrays_equal invocation_args expected_spa_residue; then
      exact_spa_residue_count=$((exact_spa_residue_count + 1))
      spa_residue_root="$invocation_root"
    fi
    token_index=$((token_index + argument_count))
  done

  [ "$invocation_count" -eq 6 ] || \
    fail "SPA selector contract allows exactly two SPA find operations and no additional find lookup; observed $invocation_count total find invocations"
  [ "$exact_spa_removal_count" -eq 1 ] || \
    fail "SPA selector contract requires exactly one literal -name libspa-*.so* removal operation; observed $exact_spa_removal_count"
  [ "$exact_spa_residue_count" -eq 1 ] || \
    fail "SPA selector contract requires exactly one literal -name libspa-*.so* residue operation; observed $exact_spa_residue_count"
  [ "$spa_removal_root" = "$spa_residue_root" ] || \
    fail "SPA removal and residue find operations used different AppDir roots"

  cmp -s -- "$FIXTURE_APPDIR/usr/lib/LIBSPA-support.so.0" \
    "$CAPTURED_APPDIR/usr/lib/LIBSPA-support.so.0" || \
    fail "case-insensitive SPA lookup changed the uppercase near-prefix control"
}

case_strips_files_symlinks_and_special_paths() {
  new_case strips_files_symlinks_and_special_paths true
  mkdir -p "$FIXTURE_APPDIR/usr/lib/spa path [*]?/support" \
    "$FIXTURE_APPDIR/usr/lib/spa path [*]?/jack" \
    "$FIXTURE_APPDIR/usr/lib/spa path [*]?/audioconvert" \
    "$FIXTURE_APPDIR/usr/lib/spa path [*]?/syntactic cases"
  touch "$FIXTURE_APPDIR/usr/lib/libglib-2.0.so.0"
  touch "$FIXTURE_APPDIR/usr/lib/libgobject-2.0.so.0"
  touch "$FIXTURE_APPDIR/usr/lib/libgio-2.0.so.0"
  touch "$FIXTURE_APPDIR/usr/lib/libgmodule-2.0.so.0"
  touch "$FIXTURE_APPDIR/usr/lib/libpipewire-0.3.so.0.1300.1"
  ln -s libpipewire-0.3.so.0.1300.1 "$FIXTURE_APPDIR/usr/lib/libpipewire-0.3.so.0"
  ln -s libpipewire-0.3.so.0 "$FIXTURE_APPDIR/usr/lib/libpipewire-0.3.so"
  touch "$FIXTURE_APPDIR/usr/lib/spa path [*]?/support/libspa-support.so.0.2.0"
  ln -s libspa-support.so.0.2.0 \
    "$FIXTURE_APPDIR/usr/lib/spa path [*]?/support/libspa-support.so.0"
  ln -s libspa-support.so.0 \
    "$FIXTURE_APPDIR/usr/lib/spa path [*]?/support/libspa-support.so"
  touch "$FIXTURE_APPDIR/usr/lib/spa path [*]?/jack/libspa-jack.so"

  # This is an authentic PipeWire SPA plugin basename. The generated matrix
  # below exercises only the syntactic libspa-<stem>.so* contract; those stems
  # are deliberately not presented as upstream plugin names.
  local authentic_spa_node='usr/lib/spa path [*]?/audioconvert/libspa-audioconvert.so'
  touch "$FIXTURE_APPDIR/$authentic_spa_node"
  [ -f "$FIXTURE_APPDIR/$authentic_spa_node" ] && \
    [ ! -L "$FIXTURE_APPDIR/$authentic_spa_node" ] || \
    fail "authentic SPA plugin fixture was not seeded: $authentic_spa_node"

  local -a syntactic_stem_heads=(alpha mix2 route)
  local -a syntactic_stem_tails=(core -bridge2 _node3 .stage4)
  local -a syntactic_suffixes=(.so .so.0 .so.7.4)
  local -a syntactic_spa_nodes=()
  local stem_head stem_tail suffix syntactic_spa_node
  local syntactic_suffix_count="${#syntactic_suffixes[@]}"
  local matrix_index=0
  for stem_head in "${syntactic_stem_heads[@]}"; do
    for stem_tail in "${syntactic_stem_tails[@]}"; do
      suffix="${syntactic_suffixes[$((matrix_index % syntactic_suffix_count))]}"
      syntactic_spa_node="usr/lib/spa path [*]?/syntactic cases/libspa-${stem_head}${stem_tail}${suffix}"
      touch "$FIXTURE_APPDIR/$syntactic_spa_node"
      syntactic_spa_nodes+=("$syntactic_spa_node")
      matrix_index=$((matrix_index + 1))
    done
  done
  touch "$FIXTURE_APPDIR/usr/lib/control file [*]?.so.1"

  local outside_dir="$CASE_ROOT/outside targets [*]?"
  mkdir -p "$outside_dir"
  printf 'outside-banned-target\n' > "$outside_dir/pipewire target"
  printf 'outside-control-target\n' > "$outside_dir/control target"
  ln -s "$outside_dir/pipewire target" "$FIXTURE_APPDIR/usr/lib/libpipewire-outside.so.0"
  ln -s "$outside_dir/control target" "$FIXTURE_APPDIR/usr/lib/control-link.so"

  run_post_processor_successfully "special-character AppImage fixture"
  assert_no_banned_libs "$CAPTURED_APPDIR"
  [ ! -e "$CAPTURED_APPDIR/$authentic_spa_node" ] && \
    [ ! -L "$CAPTURED_APPDIR/$authentic_spa_node" ] || \
    fail "authentic SPA plugin remained in the AppDir: $authentic_spa_node"
  for syntactic_spa_node in "${syntactic_spa_nodes[@]}"; do
    [ ! -e "$CAPTURED_APPDIR/$syntactic_spa_node" ] && \
      [ ! -L "$CAPTURED_APPDIR/$syntactic_spa_node" ] || \
      fail "syntactic SPA basename-class node remained in the AppDir: $syntactic_spa_node"
  done
  local banned_chain_node
  for banned_chain_node in \
    usr/lib/libpipewire-0.3.so \
    usr/lib/libpipewire-0.3.so.0 \
    usr/lib/libpipewire-0.3.so.0.1300.1 \
    'usr/lib/spa path [*]?/support/libspa-support.so' \
    'usr/lib/spa path [*]?/support/libspa-support.so.0' \
    'usr/lib/spa path [*]?/support/libspa-support.so.0.2.0' \
    'usr/lib/spa path [*]?/jack/libspa-jack.so'; do
    [ ! -e "$CAPTURED_APPDIR/$banned_chain_node" ] && \
      [ ! -L "$CAPTURED_APPDIR/$banned_chain_node" ] || \
      fail "forbidden library chain node remained in the AppDir: $banned_chain_node"
  done
  [ -f "$CAPTURED_APPDIR/usr/lib/control file [*]?.so.1" ] || \
    fail "unrelated control file in a metacharacter path was removed"
  [ -L "$CAPTURED_APPDIR/usr/lib/control-link.so" ] || \
    fail "unrelated outside-target symlink was removed"
  [ ! -e "$CAPTURED_APPDIR/usr/lib/libpipewire-outside.so.0" ] && \
    [ ! -L "$CAPTURED_APPDIR/usr/lib/libpipewire-outside.so.0" ] || \
    fail "matching PipeWire symlink remained in the AppDir"
  [ "$(<"$outside_dir/pipewire target")" = outside-banned-target ] || \
    fail "removing a matching symlink changed its outside target"
  [ "$(<"$outside_dir/control target")" = outside-control-target ] || \
    fail "post-processing changed an unrelated outside symlink target"
  [ "$(<"$EVENT_LOG")" = $'unsquashfs\nmksquashfs' ] || \
    fail "AppImage extraction/repack order was not preserved: $(<"$EVENT_LOG")"
}

case_glib_selector() {
  local case_name="$1"
  local selector_name="$2"
  new_case "$case_name"
  touch "$FIXTURE_APPDIR/usr/lib/$selector_name"
  touch "$FIXTURE_APPDIR/usr/lib/libpipewire-0.3.so.0"
  run_post_processor_successfully "GLib selector $selector_name"
  assert_no_banned_libs "$CAPTURED_APPDIR"
}

case_absent_pipewire_is_already_clean() {
  new_case absent_pipewire_is_already_clean
  touch "$FIXTURE_APPDIR/usr/lib/libglib-2.0.so.0"
  mkdir -p "$FIXTURE_APPDIR/usr/lib/spa-0.2/support"
  touch "$FIXTURE_APPDIR/usr/lib/spa-0.2/support/libspa-support.so.0"
  run_post_processor_successfully "GLib+SPA fixture with no PipeWire library"
  assert_no_banned_libs "$CAPTURED_APPDIR"
}

case_absent_spa_is_already_clean() {
  new_case absent_spa_is_already_clean
  touch "$FIXTURE_APPDIR/usr/lib/libglib-2.0.so.0"
  touch "$FIXTURE_APPDIR/usr/lib/libpipewire-0.3.so.0"
  run_post_processor_successfully "fixture with no SPA library"
  assert_no_banned_libs "$CAPTURED_APPDIR"
}

case_missing_glib_still_fails() {
  new_case missing_glib_still_fails
  touch "$FIXTURE_APPDIR/usr/lib/libpipewire-0.3.so.0"
  local output status
  if output="$(run_post_processor 2>&1)"; then
    fail "missing GLib family was accepted because another family was removed"
  else
    status=$?
  fi
  [ "$status" -ne 0 ] || fail "missing GLib family returned success"
  case "$output" in
    *"No bundled GLib libs found"*) ;;
    *) fail "missing GLib family did not produce its family-specific error: $output" ;;
  esac
}

case_extractor_failure_is_atomic() {
  new_case extractor_failure_is_atomic
  touch "$FIXTURE_APPDIR/usr/lib/libglib-2.0.so.0"
  touch "$FIXTURE_APPDIR/usr/lib/libpipewire-0.3.so.0"
  local original="$CASE_ROOT/original.AppImage"
  cp -- "$APPIMAGE" "$original"
  FAKE_UNSQUASHFS_STATUS=31
  export FAKE_UNSQUASHFS_STATUS
  local output status
  if output="$(run_post_processor 2>&1)"; then
    fail "extractor failure was reported as success"
  else
    status=$?
  fi
  [ "$status" -eq 31 ] || fail "extractor failure status 31 was not propagated; got $status: $output"
  cmp -s -- "$APPIMAGE" "$original" || fail "extractor failure changed the original AppImage"
  [ "$(<"$EVENT_LOG")" = unsquashfs-fail ] || fail "unexpected extractor failure events: $(<"$EVENT_LOG")"
}

case_repacker_failure_is_atomic() {
  new_case repacker_failure_is_atomic
  touch "$FIXTURE_APPDIR/usr/lib/libglib-2.0.so.0"
  touch "$FIXTURE_APPDIR/usr/lib/libpipewire-0.3.so.0"
  local original="$CASE_ROOT/original.AppImage"
  cp -- "$APPIMAGE" "$original"
  FAKE_MKSQUASHFS_STATUS=32
  export FAKE_MKSQUASHFS_STATUS
  local output status
  if output="$(run_post_processor 2>&1)"; then
    fail "repacker failure was reported as success"
  else
    status=$?
  fi
  [ "$status" -eq 32 ] || fail "repacker failure status 32 was not propagated; got $status: $output"
  cmp -s -- "$APPIMAGE" "$original" || fail "repacker failure changed the original AppImage"
  [ "$(<"$EVENT_LOG")" = $'unsquashfs\nmksquashfs-fail' ] || \
    fail "unexpected repacker failure events: $(<"$EVENT_LOG")"
}

prepare_public_make_case() {
  local name="$1"
  new_case "$name"
  touch "$FIXTURE_APPDIR/usr/lib/libglib-2.0.so.0"
  touch "$FIXTURE_APPDIR/usr/lib/libpipewire-0.3.so.0"

  PUBLIC_ROOT="$CASE_ROOT/public-repository"
  PUBLIC_TARGET="$CASE_ROOT/public-target"
  PUBLIC_APPIMAGE="$PUBLIC_TARGET/release/bundle/appimage/public image [*]?.AppImage"
  mkdir -p "$PUBLIC_ROOT/scripts" "$PUBLIC_ROOT/src-tauri/bin"
  cp -- "$REPO_ROOT/Makefile" "$PUBLIC_ROOT/Makefile"
  cp -- "$SCRIPT" "$PUBLIC_ROOT/scripts/fix-appimage-glib.sh"
  make_fake_appimage "$PUBLIC_APPIMAGE"
  export CARGO_TARGET_DIR="$PUBLIC_TARGET"
}

run_public_make() {
  PATH="$FAKE_BIN:$PATH" TAURI_SIGNING_PRIVATE_KEY=test-key \
    make -C "$PUBLIC_ROOT" build \
      CARGO_TARGET_DIR="$PUBLIC_TARGET" \
      CARGO="$FAKE_BIN/fake-cargo" \
      NPM="$FAKE_BIN/fake-npm"
}

case_public_make_propagates_postprocessor_failure() {
  prepare_public_make_case public_make_propagates_postprocessor_failure
  local original="$CASE_ROOT/original.AppImage"
  cp -- "$PUBLIC_APPIMAGE" "$original"
  FAKE_MKSQUASHFS_STATUS=37
  export FAKE_MKSQUASHFS_STATUS

  local output status
  if output="$(run_public_make 2>&1)"; then
    fail "public make build swallowed post-processor failure and returned success: $output"
  else
    status=$?
  fi
  [ "$status" -ne 0 ] || fail "public make build returned zero after post-processor failure"
  cmp -s -- "$PUBLIC_APPIMAGE" "$original" || \
    fail "public make failure changed the original AppImage"
  [ ! -e "$SIGNED_APPIMAGE" ] || fail "public make signed after post-processor failure"
  [ ! -e "$PUBLIC_APPIMAGE.sig" ] || fail "public make created a signature after post-processor failure"
  [ "$(<"$EVENT_LOG")" = $'cargo\nnpm-build\nunsquashfs\nmksquashfs-fail' ] || \
    fail "public make ran work after post-processor failure: $(<"$EVENT_LOG")"
}

case_public_make_signs_only_final_bytes() {
  prepare_public_make_case public_make_signs_only_final_bytes
  local output status
  if output="$(run_public_make 2>&1)"; then
    status=0
  else
    status=$?
    fail "public make success path returned status $status: $output"
  fi
  [ "$status" -eq 0 ] || fail "public make success path was nonzero"
  [ -f "$SIGNED_APPIMAGE" ] || fail "public make did not invoke signing after a successful repack"
  cmp -s -- "$PUBLIC_APPIMAGE" "$SIGNED_APPIMAGE" || \
    fail "public signing did not receive the final successful AppImage bytes"
  [ -f "$PUBLIC_APPIMAGE.sig" ] || fail "public signing did not create a signature"
  case "$(<"$PUBLIC_APPIMAGE")" in
    *FINAL-SQUASHFS-BYTES*) ;;
    *) fail "public make signed an AppImage without the successful repack bytes" ;;
  esac
  [ "$(<"$EVENT_LOG")" = $'cargo\nnpm-build\nunsquashfs\nmksquashfs\nnpm-sign' ] || \
    fail "public make did not repack before signing: $(<"$EVENT_LOG")"
}

run_case() {
  case "$1" in
    extractor_payload) case_extractor_authenticates_sliced_payload ;;
    spa_near_prefix_controls) case_spa_near_prefix_controls_survive ;;
    spa_selector_contract) case_spa_selector_contract ;;
    strips_files_symlinks_and_special_paths) case_strips_files_symlinks_and_special_paths ;;
    glib_glib) case_glib_selector glib_glib libglib-2.0.so.0 ;;
    glib_gobject) case_glib_selector glib_gobject libgobject-2.0.so.0 ;;
    glib_gio) case_glib_selector glib_gio libgio-2.0.so.0 ;;
    glib_gmodule) case_glib_selector glib_gmodule libgmodule-2.0.so.0 ;;
    absent_pipewire) case_absent_pipewire_is_already_clean ;;
    absent_spa) case_absent_spa_is_already_clean ;;
    missing_glib) case_missing_glib_still_fails ;;
    extractor_failure) case_extractor_failure_is_atomic ;;
    repacker_failure) case_repacker_failure_is_atomic ;;
    public_make_failure) case_public_make_propagates_postprocessor_failure ;;
    public_make_success) case_public_make_signs_only_final_bytes ;;
    *) fail "unknown test case: $1" ;;
  esac
  printf 'PASS: %s\n' "$1"
}

if [ "$#" -eq 0 ]; then
  set -- \
    extractor_payload spa_near_prefix_controls spa_selector_contract \
    strips_files_symlinks_and_special_paths \
    glib_glib glib_gobject glib_gio glib_gmodule \
    absent_pipewire absent_spa missing_glib \
    extractor_failure repacker_failure \
    public_make_failure public_make_success
fi

for test_case in "$@"; do
  run_case "$test_case"
done

echo "PASS: AppImage post-processing behavior suite"
