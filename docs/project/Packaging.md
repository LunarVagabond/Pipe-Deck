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
| Flatpak | Flatpak manifest + `flatpak-builder` | Sandboxed install for broader distro coverage |

### Build Outputs

```
target/release/bundle/
  deb/pipe-deck_<version>_amd64.deb
  rpm/pipe-deck-<version>.x86_64.rpm
  appimage/pipe-deck_<version>_amd64.AppImage   # optional
flatpak/
  com.pipedeck.PipeDeck.yml
  build/                                        # flatpak-builder output
```

## Config Paths

All package formats must respect the same config layout (XDG):

- Config: `~/.config/pipe-deck/`
- Override: `PIPE_DECK_CONFIG_DIR` environment variable

Installed packages must not embed user config; first run creates defaults in user config directory.

## Dependencies

Runtime dependencies (document per distro):

| Distro family | PipeWire | Session manager | Notes |
|---------------|----------|-----------------|-------|
| Debian/Ubuntu | `pipewire` | `wireplumber` | `pactl` from `pipewire-pulse` or `pulseaudio-utils` |
| Fedora/RHEL | `pipewire` | `wireplumber` | `pw-dump` from `pipewire-utils` |
| Arch | `pipewire` | `wireplumber` | Same as above |
| Flatpak | Portal/socket | Portal | Manifest uses `--socket=pipewire-pulse` |

Build dependencies:

- Rust toolchain
- Node.js 20+ (frontend build)
- Tauri system dependencies (webkit2gtk, etc. on Linux)
- Flatpak SDK (for Flatpak builds only)

### Build Commands

```bash
make check          # frontend type-check + Rust check
make test           # Rust unit tests
make build          # production bundles (deb/rpm/AppImage/binary)
```

Flatpak (local):

```bash
flatpak-builder --force-clean flatpak/build flatpak/com.pipedeck.PipeDeck.yml
```

## CI Strategy (Phase 2)

- Build matrix: binary + deb on Ubuntu; rpm on Fedora; Flatpak on generic Linux runner.
- Smoke test: install artifact, launch app, verify enumeration view loads.
- Artifact upload for manual QA; no repository publishing yet.

## Flatpak Considerations

- Sandbox may restrict direct PipeWire socket access; manifest must include:
  - `--socket=pipewire-pulse` or appropriate PipeWire portal permissions
  - Filesystem access for `~/.config/pipe-deck` (or XDG config portal)
- Evaluate PipeWire portal vs direct socket during Flatpak slice.

## Phase 4 Hardening (Future)

- systemd user service for optional daemon.
- Desktop file and icon installation.
- apt/rpm repository publishing.
- Consistent post-install behavior across distributions.
- AppStream metadata for software centers.

## Decisions

- Phase 2: baseline artifacts for dev/beta testing, not production repos.
- Config path is XDG-consistent across all package formats.
- Flatpak included in Phase 2 baseline; portal permissions validated during implementation.
