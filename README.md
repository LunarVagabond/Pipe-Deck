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

## Screenshots

> Placeholder wireframes — swap for real captures when available.

| Dashboard | Mixer | Routing |
|-----------|-------|---------|
| ![Dashboard — live audio graph](assets/images/dashboard.svg) | ![Mixer — per-app levels and mute](assets/images/mixer.svg) | ![Routing — application to output](assets/images/routing.svg) |

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

- Linux with **PipeWire** (and PulseAudio compatibility layer where needed)
- Rust (stable), Node.js 20+ for development builds

## Quick start

```bash
git clone https://github.com/LunarVagabond/Pipe-Deck.git
cd Pipe-Deck
make install   # first-time setup
make start     # run desktop app in dev mode
```

```bash
make build     # production bundles
make help      # list all commands
```

See [Contributing](https://github.com/LunarVagabond/Pipe-Deck/wiki/project/Contributing) for the full contributor workflow.

## Documentation

Product and technical docs live in the [GitHub Wiki](https://github.com/LunarVagabond/Pipe-Deck/wiki). The `docs/` directory is a git submodule pointing at the wiki repo.

| Section | Contents |
|---------|----------|
| [Home / index](https://github.com/LunarVagabond/Pipe-Deck/wiki/Home) | User-facing overview and doc map |
| [Product](https://github.com/LunarVagabond/Pipe-Deck/wiki/product/Product_Requirements) | Requirements, roadmap, decisions |
| [Architecture](https://github.com/LunarVagabond/Pipe-Deck/wiki/architecture/System_Architecture) | System and PipeWire design |
| [Specifications](https://github.com/LunarVagabond/Pipe-Deck/wiki/specs/UI_Spec) | UI, config, plugins, rule engine |
| [Project](https://github.com/LunarVagabond/Pipe-Deck/wiki/project/Contributing) | Contributing, packaging, plugins, [Releasing](https://github.com/LunarVagabond/Pipe-Deck/wiki/project/Release) |

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

If yes, see [Contributing](https://github.com/LunarVagabond/Pipe-Deck/wiki/project/Contributing) and open an issue or PR. [Plugin authors](https://github.com/LunarVagabond/Pipe-Deck/wiki/project/Plugins) should read the [Plugin API](https://github.com/LunarVagabond/Pipe-Deck/wiki/specs/Plugin_API).

## License

[MIT](LICENSE)

---

Enjoying Pipe Deck? Consider [buying me a coffee](https://www.buymeacoffee.com/lunarvagabond) ☕
