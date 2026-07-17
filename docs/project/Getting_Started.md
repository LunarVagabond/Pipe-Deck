# Getting Started

Everything you need to get Pipe Deck running from source, whether you're trying it out or setting up to contribute. For a map of the codebase and the day-to-day dev workflow once you're up and running, see [Development](../project/Development.md).

## Prerequisites

- **Linux** with **PipeWire** (a Wayland or X11 desktop with PipeWire as the active audio server — check with `pipewire --version`). Pipe Deck shells out to `pactl`, `pw-link`, `pw-dump`, and `pw-cli`, so these need to be on your `PATH`.
- **Rust** (stable) — install via [rustup](https://rustup.rs/) if you don't already have it.
- **Node.js 20+** and npm, for the frontend build.
- **Tauri's Linux system dependencies** — the exact package names vary by distro. On Debian/Ubuntu (also what CI installs):

  ```bash
  sudo apt-get install -y \
    libwebkit2gtk-4.1-dev \
    build-essential \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    patchelf
  ```

  For Fedora, Arch, and other distros, follow [Tauri's official prerequisites guide](https://tauri.app/start/prerequisites/) for the equivalent packages (WebKitGTK, AppIndicator, librsvg, and a C toolchain).

You do **not** need a real audio setup to start UI work — see mock mode below.

## Clone and install

```bash
git clone https://github.com/LunarVagabond/Pipe-Deck.git
cd Pipe-Deck
make install   # npm install
```

## Run in development mode

```bash
make start     # or: make dev — runs the desktop app (Tauri + Vite)
```

This opens the Tauri desktop shell against your live PipeWire graph.

### No PipeWire environment handy?

Set `PIPE_DECK_USE_MOCK=1` to run against a static sample graph instead of live PipeWire — useful for UI iteration in a VM, container, or any environment without a real audio stack:

```bash
PIPE_DECK_USE_MOCK=1 make dev
```

### Frontend only

If you're only touching Vue/TS and don't need the Tauri shell:

```bash
make dev-frontend
```

## Verify your setup

```bash
make check     # frontend type-check + cargo check — fast correctness pass
make test      # Rust unit tests + mock-backend integration suite
```

## Build production bundles

```bash
make build     # .deb / .rpm / AppImage / binary
make flatpak   # local Flatpak build
```

## Known dev-environment noise

`Xlib: extension "DRI2" missing on display ":1"` — a WebKitGTK/X11 warning, not a Pipe Deck bug. Common in VMs, nested/remote X sessions, or software-only graphics stacks. Cosmetic; doesn't affect functionality. If it's distracting:

```bash
WEBKIT_DISABLE_COMPOSITING_MODE=1 make start
```

## Troubleshooting

Common day-one failures, and how to collect the details the [bug report template](../../.github/ISSUE_TEMPLATE/bug_report.yml) asks for (Distro, Desktop, PipeWire version).

### `pactl`, `pw-link`, `pw-dump`, or `pw-cli` not found

Pipe Deck shells out to these commands to read and change the PipeWire graph (`pactl` and `pw-link`/`pw-dump` for enumeration and routing, `pw-cli` for the PipeWire version and effects). If any is missing from your `PATH`, enumeration or routing fails. Install the packages that provide them:

| Distro | `pactl` | `pw-link`, `pw-dump`, `pw-cli` |
|--------|---------|--------------------------------|
| Debian / Ubuntu | `pulseaudio-utils` | `pipewire-bin` |
| Fedora | `pulseaudio-utils` | `pipewire-utils` |
| Arch | `libpulse` | `pipewire` |

Check they resolve:

```bash
command -v pactl pw-link pw-dump pw-cli
```

### PipeWire is running but the app shows no devices or streams

Confirm the PipeWire user services are actually up:

```bash
systemctl --user status pipewire.service pipewire-pulse.service
```

Both should report `active (running)`. `pactl` talks to the PulseAudio-compatibility layer, so if `pipewire-pulse.service` is down the app can see nothing even while `pipewire.service` itself runs. When the backend can't enumerate the graph, Pipe Deck falls back to an empty graph and shows a "PipeWire unavailable" notice instead of crashing — that notice is the sign to check these services.

### Permission or session-bus errors (SSH, minimal WMs)

PipeWire and `systemctl --user` rely on a user session bus and `XDG_RUNTIME_DIR`. A bare SSH shell or a minimal window manager launched without a proper login session may not provide them, so the commands above fail with permission or bus-connection errors. Run Pipe Deck from a normal graphical login session, or make sure a user systemd/D-Bus session is present (for example via `loginctl enable-linger`, or by launching inside `dbus-run-session`).

### Capturing backend errors for a bug report

Pipe Deck does not write a log file — backend errors are printed to standard error. To capture them, launch from a terminal and watch stderr:

```bash
make start   # or run the built binary directly from a terminal
```

To separate a UI bug from a backend or PipeWire problem, run against the mock backend. It serves a static sample graph and never touches PipeWire:

```bash
PIPE_DECK_USE_MOCK=1 make start
```

If the problem disappears in mock mode it's in the backend or your PipeWire setup; if it persists it's in the UI.

For the template's **PipeWire version** field, use:

```bash
pw-cli --version
```

## Next steps

- Read [Development](../project/Development.md) for the codebase layout and full `make` target list.
- Read [Contributing](../../.github/CONTRIBUTING.md) before opening a PR — branch naming, commit/PR title conventions, and the docs-first workflow.
- Building a plugin instead of touching core? Start at [Plugins](../project/Plugins.md) and [Plugin API](../specs/Plugin_API.md).
