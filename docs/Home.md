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

- **Source & builds:** [github.com/pipedeck/pipe-deck](https://github.com/pipedeck/pipe-deck)
- **Contributing:** [Contributing](project/Contributing)
- **Packaging:** [Packaging](project/Packaging)
- **Plugins:** [Plugins](project/Plugins)

## Documentation map

### Product
- [Product Requirements](product/Product_Requirements)
- [Roadmap](product/Roadmap)
- [Decisions](product/Decisions)

### Architecture
- [System Architecture](architecture/System_Architecture)
- [PipeWire Design](architecture/PipeWire_Design)
- [Phase 2 Scaffold](architecture/Phase2_Scaffold)

### Specifications
- [UI Spec](specs/UI_Spec)
- [Plugin API](specs/Plugin_API)
- [Config Spec](specs/Config_Spec)
- [Rule Engine Spec](specs/Rule_Engine_Spec)

### Project
- [Contributing](project/Contributing)
- [Packaging](project/Packaging)
- [Plugins](project/Plugins)
- [Plugin Review Checklist](project/Plugin_Review_Checklist)
- [Wiki publishing](project/Wiki)

## Feature acceptance filter

Every proposed feature must answer **yes** to:

> Does this make Linux audio easier to understand and manage?

If not, refine or drop the idea before building it.
