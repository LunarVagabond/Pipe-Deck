# Pipe Deck — Linux Audio Mixer & Control Center

**Pipe Deck** is an open-source **Linux audio mixer and routing app** for **PipeWire**. It gives you one desktop control center to route application audio, adjust levels, manage virtual devices, save profiles, and automate routing with rules.

## What you can do

- **Route apps** to speakers, headphones, interfaces, or virtual sinks
- **Mix audio** with per-stream volume and mute controls
- **Save profiles** as YAML and restore your setup after reboot
- **Create virtual devices** without `pw-cli` or module commands
- **Automate routing** with priority-based rules and simulation
- **See the full graph** on a live dashboard of devices and streams

## Who it's for

- **Everyday Linux users** who want audio to work without reading man pages
- **Gamers** switching between headset and speakers
- **Streamers and creators** who need repeatable multi-app routing
- **Power users** who want visibility, control, and scriptable profiles

## How it compares

| Need | Common tools | Pipe Deck |
|------|--------------|-----------|
| Volume / mute | pavucontrol, desktop applets | Built-in mixer panel |
| Per-app routing | pavucontrol, qpwgraph | Routing matrix + dashboard |
| Saved layouts | shell scripts, dotfiles | YAML profiles |
| Virtual sinks | pw-cli, null-sink modules | Guided virtual device UI |
| Automation | custom hooks | Rule engine |

Pipe Deck is **PipeWire-first** and does not replace PipeWire or WirePlumber — it sits on top and makes them easier to use.

## Get started

- **Run it from source:** [Getting Started](project/Getting_Started.md) — prerequisites, clone, first run
- **Source & builds:** [github.com/LunarVagabond/Pipe-Deck](https://github.com/LunarVagabond/Pipe-Deck)
- **Why this exists:** [About](product/About.md)
- **Contributing:** [`.github/CONTRIBUTING.md`](../.github/CONTRIBUTING.md)
- **Codebase layout:** [Development](project/Development.md)
- **Packaging:** [Packaging](project/Packaging.md)
- **Plugins:** [Plugins](project/Plugins.md)
- **Releasing:** [Release](project/Release.md)
- **Project organization:** [Project Management](project-management/README.md)

## Documentation map

### Product
- [Product Requirements](product/Product_Requirements.md)
- [Roadmap](product/Roadmap.md)
- [Decisions](architecture/Decisions.md)

### Getting Started
- [Getting Started](project/Getting_Started.md)
- [Development](project/Development.md)

### Architecture
- [System Architecture](architecture/System_Architecture.md)
- [PipeWire Design](architecture/PipeWire_Design.md)

### Specifications
- [UI Spec](specs/UI_Spec.md)
- [Theming](specs/Theming.md)
- [Plugin API](specs/Plugin_API.md)
- [Config Spec](specs/Config_Spec.md)
- [Rule Engine Spec](specs/Rule_Engine_Spec.md)
- [Audio Terminology](specs/Audio_Terminology.md)

### Project
- [Packaging](project/Packaging.md)
- [Release process](project/Release.md)
- [Plugins](project/Plugins.md)
- [Plugin Review Checklist](specs/Plugin_Review_Checklist.md)

### Project Management
- [Overview](project-management/README.md)
- [Issue Workflow](project-management/issue-workflow.md)
- [Milestones & Releases](project-management/milestones-and-releases.md)
- [Release Strategy & Cadence](project-management/release-strategy.md)
- [Contributing Workflow](project-management/contributing-workflow.md)

## Feature acceptance filter

Every proposed feature must answer **yes** to:

> Does this make Linux audio easier to understand and manage?

If not, refine or drop the idea before building it.
