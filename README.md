# Pipe Deck

**Open-source Linux audio mixer and routing control center for PipeWire.**

Pipe Deck is a desktop app that helps you **see, route, mix, and automate** Linux audio without learning PipeWire internals. Route applications to speakers or headphones, adjust levels from one mixer panel, save setups as profiles, and restore them across sessions.

> Looking for a **Linux audio mixer** that goes beyond volume sliders? Pipe Deck combines routing, mixing, virtual devices, and rule-based automation in one place — built for PipeWire on modern Linux desktops.

[![Build](https://github.com/LunarVagabond/Pipe-Deck/actions/workflows/build.yml/badge.svg)](https://github.com/LunarVagabond/Pipe-Deck/actions/workflows/build.yml)

## Why Pipe Deck

Linux audio is powerful but scattered. Routine tasks often mean juggling multiple tools:

| Task | Typical tools today | With Pipe Deck |
|------|---------------------|----------------|
| Per-app output routing | `pavucontrol`, `qpwgraph` | Routing matrix + live dashboard |
| Volume and mute | `pavucontrol`, desktop applets | Unified mixer panel |
| Saved setups | Manual scripts, dotfiles | YAML profiles — save, swap, export |
| Virtual sinks/sources | `pw-cli`, `module-null-sink` | Guided virtual device workflows |
| Automation | Custom shell hooks | Rule engine with simulation |

Pipe Deck is **PipeWire-first**, **Linux-native**, and designed so changes are **visible, reversible, and safe**.

> Curious why this project exists? Read the [full story](docs/product/About.md).

## Screenshots

> Real app screenshots.

| Dashboard | Mixer | Routing |
|-----------|-------|---------|
| ![Dashboard — live audio graph](docs/images/dashboard.png) | ![Mixer — per-app levels and mute](docs/images/mixer.png) | ![Routing — application to output](docs/images/routing.png) |

## Features

- **Live audio dashboard** — See devices, streams, and links in a normalized runtime graph.
- **Application routing** — Send any app to the sink or source you want.
- **Mixer controls** — Per-channel levels and mute from a single panel.
- **Profiles** — Save known-good YAML setups; swap or restore across reboots.
- **Virtual devices** — Create and manage virtual sinks and sources without low-level commands.
- **Rules and automation** — Priority-based routing policies with simulation before apply.
- **Plugin ecosystem** — Extend behavior via isolated JSON-RPC plugins.
- **Packaging** — Build targets for binary, `.deb`, `.rpm`, and Flatpak.

## Requirements

- Linux with **PipeWire** (and PulseAudio compatibility layer where needed) — `pactl`, `pw-link`, and `pw-dump` must be on your `PATH`
- **Rust** (stable) — via [rustup](https://rustup.rs/)
- **Node.js 20+** and npm
- Tauri's Linux system dependencies. On Debian/Ubuntu (also what CI installs):

  ```bash
  sudo apt-get install -y \
    libwebkit2gtk-4.1-dev \
    build-essential \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    patchelf
  ```

  Other distros: see [Tauri's prerequisites guide](https://tauri.app/start/prerequisites/) for the equivalent packages.

## Quick start

```bash
git clone https://github.com/LunarVagabond/Pipe-Deck.git
cd Pipe-Deck
make install   # first-time setup
make start     # run desktop app in dev mode
```

No PipeWire environment handy? `PIPE_DECK_USE_MOCK=1 make start` runs against a static sample graph instead of live PipeWire — useful for UI work in a VM or container.

```bash
make check     # frontend type-check + cargo check
make test      # Rust unit tests
make build     # production bundles
make help      # list all commands
```

Full setup walkthrough: [Getting Started](docs/project/Getting_Started.md). Contributor workflow: [Contributing](.github/CONTRIBUTING.md).

## Documentation

Product and technical docs live in [`docs/`](docs/README.md), organized by section.

| Section | Contents |
|---------|----------|
| [Docs index](docs/README.md) | User-facing overview and doc map |
| [Getting Started](docs/project/Getting_Started.md) | Prerequisites, first run, and [Development](docs/project/Development.md) codebase layout |
| [Product](docs/product/Product_Requirements.md) | Requirements, roadmap, decisions |
| [Architecture](docs/architecture/System_Architecture.md) | System and PipeWire design |
| [Specifications](docs/specs/UI_Spec.md) | UI, config, plugins, rule engine |
| [Project](docs/project/Development.md) | Packaging, plugins, [Releasing](docs/project/Release.md) — see [Contributing](.github/CONTRIBUTING.md) for the contributor workflow |

Open work is tracked in [GitHub Issues](https://github.com/LunarVagabond/Pipe-Deck/issues). List locally with `gh issue list`.

## Related projects

Pipe Deck complements — not replaces — the PipeWire stack. You may also use:

- [PipeWire](https://pipewire.org/) — session and audio graph
- [WirePlumber](https://gitlab.freedesktop.org/pipewire/wireplumber) — session manager
- [qpwgraph](https://gitlab.freedesktop.org/rncbc/qpwgraph) — node graph editor
- [pavucontrol](https://freedesktop.org/software/pulseaudio/pavucontrol/) — classic PulseAudio/PipeWire volume UI

Pipe Deck focuses on **routing clarity, profile management, and automation** in one desktop control center.

## Contributing

Every feature must pass one question:

> Does this make Linux audio easier to understand and manage?

If yes, see [Contributing](.github/CONTRIBUTING.md) and open an issue or PR. [Plugin authors](docs/project/Plugins.md) should read the [Plugin API](docs/specs/Plugin_API.md).

## Community

- [GitHub Discussions](https://github.com/LunarVagabond/Pipe-Deck/discussions) — design questions, proposals, and anything worth keeping searchable
- [Discord](https://discord.gg/SG23W3BqCn) — "Dev Syndicate" server, casual chat and quick questions

## License

[MIT](LICENSE)

---

Enjoying Pipe Deck? Consider [buying me a coffee](https://www.buymeacoffee.com/lunarvagabond) ☕
