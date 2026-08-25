#!/usr/bin/env bash
# Strip linuxdeploy's bundled host-provided libraries from a built AppImage.
#
# Issue #349: linuxdeploy's gtk plugin bundles libglib-2.0/libgobject-2.0/
# libgio-2.0/libgmodule-2.0 into the AppDir (they're not on upstream's
# excludelist the way libEGL/libGL/Mesa are), then AppRun points
# LD_LIBRARY_PATH at the AppDir first. On hosts whose system GLib is newer
# than the one bundled at build time, WebKitGTK's GDK/EGL context setup runs
# through that mismatched GObject/GLib and aborts with
# "Could not create default EGL display: EGL_BAD_PARAMETER" (a malformed
# attribute-list error, not a missing-driver one). Removing the bundled
# copies is safe: LD_LIBRARY_PATH only adds AppDir/usr/lib as an extra
# search dir ahead of the system default path — if a lib isn't there, the
# dynamic linker falls through to the host's copy, the same way the
# excludelist already makes EGL/GL/Mesa resolve against the host.
#
# Issue #299: the daemon sidecar makes linuxdeploy bundle libpipewire/libspa.
# Pipe Deck must use the host's PipeWire session and libraries, so keeping
# those copies in AppDir can make subprocesses resolve an incompatible bundle.
set -euo pipefail

APPIMAGE="${1:?usage: fix-appimage-glib.sh <path-to.AppImage>}"
[ -f "$APPIMAGE" ] || { echo "No such file: $APPIMAGE" >&2; exit 1; }

for tool in unsquashfs mksquashfs; do
  command -v "$tool" >/dev/null || { echo "Missing required tool: $tool (install squashfs-tools)" >&2; exit 1; }
done

WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

chmod +x "$APPIMAGE"
offset="$("$APPIMAGE" --appimage-offset)"

dd if="$APPIMAGE" of="$WORKDIR/runtime" bs=1 count="$offset" status=none
dd if="$APPIMAGE" of="$WORKDIR/fs.squashfs" bs=1 skip="$offset" status=none

unsquashfs -d "$WORKDIR/AppDir" "$WORKDIR/fs.squashfs" >/dev/null

strip_family() {
  local label="$1"
  local required="$2"
  shift 2

  local -a name_expression=()
  local pattern
  for pattern in "$@"; do
    if [ "${#name_expression[@]}" -gt 0 ]; then
      name_expression+=(-o)
    fi
    name_expression+=(-name "$pattern")
  done

  local removed=0
  local lib
  while IFS= read -r -d '' lib; do
    echo "Removing bundled lib: ${lib#"$WORKDIR/AppDir/"}"
    rm -f "$lib"
    removed=$((removed + 1))
  done < <(find "$WORKDIR/AppDir" \( -type f -o -type l \) \
    \( "${name_expression[@]}" \) -print0)

  if [ "$required" = required ] && [ "$removed" -eq 0 ]; then
    echo "No bundled $label libs found in $APPIMAGE — nothing to strip." >&2
    exit 1
  fi

  local remaining
  remaining="$(find "$WORKDIR/AppDir" \( -type f -o -type l \) \
    \( "${name_expression[@]}" \) -print -quit)"
  if [ -n "$remaining" ]; then
    echo "Bundled $label lib remains after stripping: ${remaining#"$WORKDIR/AppDir/"}" >&2
    exit 1
  fi

  echo "Stripped $removed bundled $label lib(s) from $APPIMAGE"
}

strip_family GLib required \
  'libglib-2.0.so*' 'libgobject-2.0.so*' 'libgio-2.0.so*' 'libgmodule-2.0.so*'
strip_family PipeWire required 'libpipewire*.so*'
strip_family SPA optional 'libspa*.so*'

mksquashfs "$WORKDIR/AppDir" "$WORKDIR/new.squashfs" -root-owned -noappend >/dev/null

cat "$WORKDIR/runtime" "$WORKDIR/new.squashfs" > "$APPIMAGE"
chmod +x "$APPIMAGE"

echo "Finished stripping bundled host libraries from $APPIMAGE"
