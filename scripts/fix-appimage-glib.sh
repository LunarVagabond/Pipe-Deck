#!/usr/bin/env bash
# Strip linuxdeploy's bundled GLib family libs from a built AppImage.
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

removed=0
while IFS= read -r -d '' lib; do
  echo "Removing bundled lib: ${lib#"$WORKDIR/AppDir/"}"
  rm -f "$lib"
  removed=$((removed + 1))
done < <(find "$WORKDIR/AppDir" -maxdepth 4 -type f \
  \( -name 'libglib-2.0.so*' -o -name 'libgobject-2.0.so*' \
     -o -name 'libgio-2.0.so*' -o -name 'libgmodule-2.0.so*' \) -print0)

if [ "$removed" -eq 0 ]; then
  echo "No bundled GLib libs found in $APPIMAGE — nothing to strip." >&2
  exit 1
fi

mksquashfs "$WORKDIR/AppDir" "$WORKDIR/new.squashfs" -root-owned -noappend >/dev/null

cat "$WORKDIR/runtime" "$WORKDIR/new.squashfs" > "$APPIMAGE"
chmod +x "$APPIMAGE"

echo "Stripped $removed bundled GLib lib(s) from $APPIMAGE"
