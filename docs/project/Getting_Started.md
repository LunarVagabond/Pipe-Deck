# Getting Started

Everything you need to get Pipe Deck running from source, whether you're trying it out or setting up to contribute. For a map of the codebase and the day-to-day dev workflow once you're up and running, see [Development](../project/Development.md).

## Prerequisites

- **Linux** with **PipeWire** (a Wayland or X11 desktop with PipeWire as the active audio server — check with `pipewire --version`). Pipe Deck shells out to `pactl`, `pw-link`, and `pw-dump`, so these need to be on your `PATH`.
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

## Next steps

- Read [Development](../project/Development.md) for the codebase layout and full `make` target list.
- Read [Contributing](../../.github/CONTRIBUTING.md) before opening a PR — branch naming, commit/PR title conventions, and the docs-first workflow.
- Building a plugin instead of touching core? Start at [Plugins](../project/Plugins.md) and [Plugin API](../specs/Plugin_API.md).
