# Development

## Purpose

Map the current codebase layout and day-to-day dev workflow so contributors can find code quickly. For first-time setup (prerequisites, clone, first run), see [Getting Started](../project/Getting_Started.md).

## Stack

- **Shell:** Tauri 2 (Rust)
- **UI:** Vue 3 + TypeScript
- **Styles:** SCSS partials under `src/styles/` (no `<style>` blocks in `.vue` files)
- **Config:** YAML under `~/.config/pipe-deck/` (XDG), override via `PIPE_DECK_CONFIG_DIR`

## Repository Layout

```
src/                         Vue 3 + TS frontend
  views/                     One per sidebar destination (Dashboard, Routing, Mixer, Sources, ...)
  components/                Vue components; routing-graph/ holds the graph-view logic
  composables/                Shared reactive logic (e.g. update checks)
  stores/                    Small composable-based stores (runtimeGraph, notices, confirm, prompt, profiles)
  styles/                    SCSS entry and partials (views/, components/)
  types/                     TypeScript domain types (graph.ts mirrors the Rust models by hand)
  utils/                     Shared frontend helpers
  e2e/                       Playwright component tests

src-tauri/src/
  commands/                  Thin Tauri command handlers — argument marshalling only, no logic
  core/
    engine/                  CoreEngine, split by domain: routing_ops, mixer_ops, virtual_ops, effects_ops, profile_ops, graph_sync
    models.rs                 Domain types shared with the frontend (Device, Stream, RuntimeGraph, ...)
    restore.rs                 Recreates configured virtual devices against PipeWire at startup/profile-swap
    rules/                     Rule matching, evaluation, manual-override detection (routing automation)
  backend/                   Platform-neutral AudioBackend trait boundary
    mod.rs                     AudioBackend trait, BackendError, create_backend() factory
    mock.rs                    Stateful mock backend for PIPE_DECK_USE_MOCK=1 and tests
    stub.rs                    Proof-of-concept second impl, wired in on non-Linux targets
    linux/                     The only real implementation today (LinuxPipeWireBackend + pactl/pw-link/pw-dump plumbing)
  pipewire/                  Effects/filter-chain plumbing (separate from the AudioBackend boundary)
  config/                    YAML config/profile persistence
  plugins/                   JSON-RPC-over-stdio plugin host
  daemon/                    Optional background restore daemon (systemd user service)
  bin/                       Separate pipe-deck-daemon and pipe-deck-cli binaries alongside the Tauri GUI
```

Two guiding rules hold this structure together: the UI never talks to PipeWire directly (every state-changing action flows UI → Tauri command → `CoreEngine` → `AudioBackend`), and engine/core code never names `backend::linux` directly, only `&dyn AudioBackend` (see `backend/mod.rs`) — this keeps the door open for a second platform backend without touching engine call sites.

## Control Flow

1. UI invokes a Tauri command (`set_stream_target`, `swap_profile`, etc.).
2. `CoreEngine` validates and applies through the `AudioBackend` trait.
3. Engine refreshes the normalized `RuntimeGraph` and emits `graph-updated`.
4. UI re-renders the dashboard matrix, mixer, and other views from the graph.

## Development Entry Points

- `make dev` / `make start` — run the desktop app (Tauri + Vite)
- `make dev-frontend` — Vite frontend only, no Tauri shell
- `make check` — frontend type-check + `cargo check`
- `make test` — Rust unit tests, plus the mock-backend end-to-end suite (`src-tauri/tests/mock_backend_integration.rs`)
- `make test-e2e` — Playwright component tests (`src/e2e/`)
- `make smoke` — install and compile smoke checks
- `make build` — production bundles (`.deb`/`.rpm`/AppImage/binary)
- `PIPE_DECK_USE_MOCK=1 make dev` — run the UI against a static sample graph instead of live PipeWire

Run `make help` for the full, current list — it stays more up to date than any doc.

## Testing Notes

`cargo test` runs the mock-backed unit tests plus the mock-backend integration suite — extend that suite rather than writing a throwaway verification script when changing engine/backend call paths.

`src-tauri/src/core/restore.rs` (session restore, profile-swap virtual-device restore, persisted-route reapplication) is covered the same way: its functions take `&dyn AudioBackend` directly, so `mock_backend_integration.rs`'s `restore_*` tests exercise them against a bare `MockAudioBackend` (device create/adopt/orphan-removal, profile `device_assumptions` restore) rather than through `CoreEngine`, which never reaches `restore.rs` at all in mock mode.

`cargo test` shares global process state (e.g. `PIPE_DECK_CONFIG_DIR`) across tests and can flake under the default parallel runner; if a `config`/`routing_rules` test fails in isolation, rerun with `-- --test-threads=1` before assuming it's a real regression.

## Related Documents

- [Getting Started](../project/Getting_Started.md)
- [System Architecture](../architecture/System_Architecture.md)
- [PipeWire Design](../architecture/PipeWire_Design.md)
- [Config Spec](../specs/Config_Spec.md)
- [UI Spec](../specs/UI_Spec.md)
- [GitHub Issues](https://github.com/LunarVagabond/Pipe-Deck/issues) (open work tracker)
