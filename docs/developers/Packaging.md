# Packaging

## Purpose

Define Phase 2 packaging targets and baseline distribution strategy for Pipe Deck.

## In Scope

- Target package formats and build outputs.
- Config path conventions across install methods.
- Phase 2 vs Phase 4 packaging maturity.

## Out of Scope

- Production apt/rpm repository hosting.
- Code signing and notarization.
- App store distribution.

## Phase 2 Packaging Baseline

Phase 2 delivers **installable dev/beta artifacts** for validation. Production hardening (systemd units, desktop integration polish, repository publishing) is Phase 4.

### Target Formats

| Format | Tooling | Phase 2 Goal |
|--------|---------|--------------|
| Binary | `cargo tauri build` | Standalone executable for local testing |
| `.deb` | `cargo tauri build` (deb bundle) or `cargo-deb` | Installable on Debian/Ubuntu derivatives |
| `.rpm` | `cargo-generate-rpm` or equivalent | Installable on Fedora/RHEL derivatives |

### Build Outputs

```
target/release/bundle/
  deb/pipe-deck_<version>_amd64.deb
  rpm/pipe-deck-<version>.x86_64.rpm
  appimage/pipe-deck_<version>_amd64.AppImage   # optional
```

## Config Paths

All package formats must respect the same config layout (XDG):

- Config: `~/.config/pipe-deck/`
- Override: `PIPE_DECK_CONFIG_DIR` environment variable

Installed packages must not embed user config; first run creates defaults in user config directory.

Removing the package does not remove user config, live-effects drop-ins, or the optional daemon's systemd unit — see [Uninstall](Uninstall.md) for the full inventory and cleanup steps.

## Dependencies

Runtime dependencies (document per distro):

| Distro family | PipeWire | Session manager | Notes |
|---------------|----------|-----------------|-------|
| Debian/Ubuntu | `pipewire` | `wireplumber` | `pactl` from `pipewire-pulse` or `pulseaudio-utils` |
| Fedora/RHEL | `pipewire` | `wireplumber` | `pw-dump` from `pipewire-utils` |
| Arch | `pipewire` | `wireplumber` | Same as above |

Build dependencies:

- Rust toolchain
- Node.js 20+ (frontend build)
- Tauri system dependencies (webkit2gtk, etc. on Linux)

### Build Commands

```bash
make check          # frontend type-check + Rust check + clippy
make test           # Rust unit tests
make build          # production bundles (deb/rpm/AppImage/binary)
```

## CI Strategy (Phase 2)

- Build matrix: binary + deb on Ubuntu; rpm on Fedora.
- Smoke test: install artifact, launch app, verify enumeration view loads.
- Artifact upload for manual QA; no repository publishing yet.

## Phase 4 Hardening

- systemd user service for optional daemon (`pipe-deck-daemon.service`).
- Desktop file and AppStream metadata in `packaging/`.
- Runtime dependencies declared in Tauri bundle config.
- `make smoke` for local validation.
- CI: smoke checks, Ubuntu bundles, RPM build.

### Daemon install paths

| Format | Daemon binary | systemd unit |
|--------|---------------|--------------|
| `.deb` | `/usr/bin/pipe-deck-daemon` (external bin) | `/usr/lib/systemd/user/pipe-deck-daemon.service` |
| Dev build | `src-tauri/target/release/pipe-deck-daemon` | `~/.config/systemd/user/pipe-deck-daemon.service` (via Settings UI) |

Enable background restore from the in-app **Settings** view (installs user unit, runs `systemctl --user enable --now`).

### AppImage GLib patch (issue #349)

`make build`'s AppImage step is followed by `scripts/fix-appimage-glib.sh`, which strips linuxdeploy's bundled `libglib-2.0`/`libgobject-2.0`/`libgio-2.0`/`libgmodule-2.0` out of the `.AppDir` before it's repackaged. Those libs aren't on upstream's `probonopd/AppImages` excludelist the way `libEGL`/`libGL`/Mesa are, so linuxdeploy bundles them; AppRun then points `LD_LIBRARY_PATH` at the AppDir ahead of the system path, and on a host whose system GLib is newer than the one bundled at build time, WebKitGTK's GDK/EGL context setup runs through that mismatched GObject/GLib and aborts with `Could not create default EGL display: EGL_BAD_PARAMETER` — a malformed-attribute error, not a missing-driver one, and the AppImage is the only affected format (the `.deb` links the host's own webkit2gtk). Removing the bundled copies is safe: LD_LIBRARY_PATH only adds a search dir ahead of the default path, so if a lib isn't there the dynamic linker falls through to the host's copy, same as already happens for EGL/GL.

The script requires `unsquashfs`/`mksquashfs` (`squashfs-tools`); if they're not installed, `make build` logs a warning and skips the patch rather than failing — CI installs `squashfs-tools` explicitly so release builds are always patched. Since the patch mutates the `.AppImage` after Tauri's own build-time updater signing, `make build` re-signs it via `tauri signer sign` whenever `TAURI_SIGNING_PRIVATE_KEY`(`_PATH`) is set (as it is in CI); unsigned local dev builds just get the patch with no `.sig`.

### Uninstalling

Tauri's bundler has no `postrm`/`postinst`/`%postun` hook config for the `.deb`/`.rpm` targets above — package removal only removes package-owned files (the binary, desktop file, the `/usr/lib/systemd/user/` unit template), never the per-user state Pipe Deck writes at runtime (config, the *enabled* systemd unit under `~/.config/systemd/user/`, live `pactl` virtual device modules). `pipe-deck-cli cleanup [--purge-config]` (issue #169) is the explicit, scriptable answer until/unless a future packaging change adds real pre/post-removal hooks; see `docs/developers/Uninstall.md` for the full breakdown and the Flatpak sandboxing caveat.

## Decisions

- Phase 2: baseline artifacts for dev/beta testing, not production repos.
- Config path is XDG-consistent across all package formats.
