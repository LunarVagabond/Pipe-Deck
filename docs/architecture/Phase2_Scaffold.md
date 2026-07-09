# Phase 2 Scaffold

## Purpose

Map the Phase 2 implementation layout so contributors can find code quickly.

## Stack

- **Shell:** Tauri 2 (Rust)
- **UI:** Vue 3 + TypeScript
- **Styles:** SCSS partials under `src/styles/` (no `<style>` in `.vue` files)
- **Config:** YAML under `~/.config/pipe-deck/` (XDG)

## Repository Layout

```
src/                          # Vue frontend
  components/                 # Dashboard matrix, mixer, shared cards
  views/                      # Dashboard and other views
  stores/                     # Runtime graph and config state
  styles/                     # SCSS entry and component styles
  types/                      # TypeScript domain types

src-tauri/src/
  commands/                   # Tauri IPC commands (thin wrappers)
  config/                     # YAML load/save, profile store
  core/
    engine.rs                 # Orchestration, graph refresh, apply/rollback
    routing.rs                # Routing intents and validation
    routing_rules.rs          # Lightweight persisted route re-apply
    profile.rs                # Profile capture helpers
    models.rs                 # Shared domain types
  pipewire/
    adapter.rs                # PipeWire adapter trait
    live.rs                   # pw-dump graph, pactl enrichment, visual links
    pactl.rs                  # move-sink-input, virtual devices, feed sinks
    pw_link.rs                # Device-to-device monitor routing
    virtual_devices.rs        # Virtual device registry
    mock.rs                   # Mock graph for PIPE_DECK_USE_MOCK=1
```

## Control Flow

1. UI invokes a Tauri command (`set_stream_target`, `swap_profile`, etc.).
2. `CoreEngine` validates and applies through the PipeWire adapter.
3. Engine refreshes the normalized `RuntimeGraph` and emits `graph-updated`.
4. UI re-renders dashboard matrix and mixer from the graph.

## Development Entry Points

- `make dev` — run the desktop app
- `make test` — Rust unit tests
- `make check` — format, lint, and type checks
- `PIPE_DECK_USE_MOCK=1 make dev` — UI without live PipeWire

## Related Documents

- `docs/architecture/System_Architecture.md`
- `docs/architecture/PipeWire_Design.md`
- `docs/specs/Config_Spec.md`
- `docs/specs/UI_Spec.md`
- `Backlog.md`
