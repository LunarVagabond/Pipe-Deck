# Config Spec

## Purpose

Define how Pipe Deck stores user settings, profiles, and policy state in a format that is safe, understandable, and maintainable.

## In Scope

- Config document structure and versioning.
- Defaults and override model.
- Profile persistence and swap semantics.
- Storage paths and export/import behavior.

## Out of Scope

- SQLite or other database backends (deferred; YAML remains the portable contract).
- Distribution-specific packaging install paths.

## Config Design Principles

- Human-readable YAML where practical.
- Safe defaults for new users.
- Explicit schema versioning.
- Backward-compatible migration path.
- File-first: profiles as separate files; main config holds index and active pointer.

## Storage Layout

Default path follows XDG Base Directory spec:

```
~/.config/pipe-deck/
  config.yaml          # main config: preferences, active profile, profile index
  profiles/
    gaming.yaml        # one file per saved profile
    streaming.yaml
    default.yaml
```

Environment override: `PIPE_DECK_CONFIG_DIR` (optional).

## Serialization Format

- **Format:** YAML
- **Schema version:** required in every config and profile file
- **Rationale:** human-readable, easy to edit, copy, and version-control; portable across machines

## Top-Level Configuration Model (`config.yaml`)

```yaml
version: 1
preferences:
  show_system_streams: false
  restore_on_startup: true       # recreate virtual devices when GUI opens
  background_restore: false      # enable systemd daemon at login (Phase 4)
  theme_mode: dark                # "light" | "dark" | "system"
  dark_scheme: midnight-deck      # scheme id used when resolved mode is dark
  light_scheme: paper-deck        # scheme id used when resolved mode is light
  notice_duration_ms: 5000        # toast auto-dismiss delay; 0 means "until dismissed"
active_profile: gaming
profile_index:
  - id: gaming
    name: Gaming
    file: profiles/gaming.yaml
  - id: streaming
    name: Streaming
    file: profiles/streaming.yaml
devices: {}        # known device metadata and aliases
virtual_devices:   # persisted virtual device definitions (Phase 4)
  - id: virtual-game-mix
    slug: game-mix
    label: Game Mix
    direction: output
    created_at: "2026-07-09T10:00:00Z"
routing_rules:     # lightweight persisted routes (Phase 2); see below
  stream_rules: []
  device_rules: []
rules: []          # authored auto-routing rules (Phase 3+)
plugins:           # plugin enablement and capability grants (Phase 5)
  pipe-deck-effects:
    enabled: true
    granted_capabilities:
      - graph.read
      - effects.manage
      - ui.panel.register
    config:
      chains: {}
diagnostics:
  verbosity: normal
```

### Plugins (`plugins`, Phase 5)

Map of plugin ID → runtime state. See [Plugin API](../specs/Plugin_API.md).

```yaml
plugins:
  pipe-deck-effects:
    enabled: true
    granted_capabilities:
      - graph.read
      - effects.manage
      - ui.panel.register
    config:              # opaque per-plugin config blob
      chains:
        virtual-game-mix:
          eq_low: 0
          eq_mid: 0
          eq_high: 0
          compressor: false
  my-community-plugin:
    enabled: false
    granted_capabilities: []
    config: {}
```

| Field | Description |
|-------|-------------|
| `enabled` | Whether the host should start this plugin |
| `granted_capabilities` | User-approved subset of manifest `capabilities` |
| `config` | Plugin-owned settings persisted by the host |

Bundled first-party plugins ship with `enabled: true` and default grants on first run.

### Multi-output routing (Phase 5)

Stream routes may target multiple outputs simultaneously. Profiles and rules use `target_device_ids` (array); legacy `target_device_id` (single string) is still accepted on load.

```yaml
routing_intents:
  - stream_id: firefox-playback
    target_device_ids:
      - headphones
      - desk-speakers
```

Fan-out uses a `pipe-deck-split-*` virtual sink plus `pw-link` monitor routes to each output.

### Virtual devices (`virtual_devices[]`, Phase 4)

Each entry describes a Pipe Deck-owned virtual sink or source that should be recreated after reboot or session restart:

- **id:** stable internal ID (`virtual-{slug}`)
- **slug:** suffix used in PipeWire node name `pipe-deck-{slug}`
- **label:** user-facing name
- **direction:** `output` or `input`
- **created_at:** ISO 8601 timestamp

On first run after upgrade, existing `pipe-deck-*` PipeWire modules are migrated into this list automatically.

### Restore preferences

- **restore_on_startup:** when true, the GUI (or daemon) recreates missing virtual devices and reapplies persisted routes on start.
- **background_restore:** when true, the optional `pipe-deck-daemon` user service is enabled for login-time restore.

### Appearance / theme preferences

- **theme_mode:** `"light"`, `"dark"`, or `"system"` (follows `prefers-color-scheme`). Defaults to `"dark"` so existing installs keep today's look.
- **dark_scheme** / **light_scheme:** ids of the color scheme applied when the resolved mode is dark or light, respectively — selected independently, so a user can pair any dark scheme with any light scheme. Default to the built-in `midnight-deck` and `paper-deck`.
- **notice_duration_ms:** how long a toast notice (route applied, profile saved, errors, ...) stays on screen before auto-dismissing, in milliseconds. `0` means the notice stays until manually dismissed. Defaults to `5000` (5 seconds), matching prior hardcoded behavior.
- Scheme ids resolve against the 4 built-in schemes plus any user-authored custom schemes under `<config_dir>/themes/*.yaml`. If a selected custom scheme's file is missing (deleted, moved), the app falls back to the built-in default for that mode. See [Theming](../specs/Theming.md) for the scheme file schema and fallback/merge rules.

### Daemon status and safe mode (Phase 4)

- **Status file:** `~/.local/state/pipe-deck/daemon.json` (`pid`, `last_run`, `last_error`, `devices_restored`)
- **Safe mode:** if config is missing or corrupt on daemon start, the daemon records the error and exits without modifying PipeWire

## Profile Format (`profiles/<name>.yaml`)

Each profile captures a desired-state snapshot:

```yaml
version: 1
id: gaming
name: Gaming
created: "2026-07-09T10:00:00Z"
updated: "2026-07-09T10:00:00Z"
routing_intents:
  - stream_id: "firefox-playback"
    target_sink: "headphones"
  - stream_id: "discord-playback"
    target_sink: "virtual-game-mix"
volume_state: {}   # optional per-device/stream levels and mute
device_assumptions: {}  # optional expected device presence
rule_overrides: []      # optional (Phase 3+)
```

### Profile Fields

- **Metadata:** `id`, `name`, `created`, `updated`
- **Routing intents:** stream → sink/source target mappings
- **Volume/mute state:** optional capture of levels when saving
- **Device assumptions:** virtual device IDs present when the profile was saved; used to restore missing devices before applying routing intents
- **Rule overrides:** optional rule activation overrides (Phase 3+)

## Routing Rules (`config.yaml`)

Pipe Deck uses two complementary persistence layers for automatic routing.

### Authored rules (`rules[]`, Phase 3+)

User-defined policies with priority, conditions, enable/disable, and simulation. Evaluated by the rule engine in `src-tauri/src/core/rules/`. See [Rule Engine Spec](../specs/Rule_Engine_Spec.md).

```yaml
rules:
  - id: firefox-hdmi
    name: Firefox to HDMI
    enabled: true
    priority: 10
    conditions:
      - type: executable
        value: firefox
    action:
      target_system_name: hdmi
    safeguards:
      fallback_policy: keep_current
```

#### Condition types

| `type` | Fields | Notes |
|--------|--------|-------|
| `app_name` | `value` | PipeWire `application.name` |
| `executable` | `value` | Process binary |
| `media_name` | `value` | Disambiguates multiple streams per app |
| `window_class` | `value` | Best-effort: `window.x11.class`, else `application.id`, else `application.icon-name`. Matches case-insensitively and tolerates a reverse-DNS `application.id` on either side (`org.mozilla.firefox` matches `firefox`) |
| `direction` | `value` | `playback` or `capture` |
| `category` | `value` | Heuristic bucket (`Game`, `Music`, `Chat`, etc.) |
| `regex` | `field`, `pattern` | Match on `app_name`, `executable`, `media_name`, or `window_class` |

#### Evaluation precedence

1. Session manual overrides (dashboard picks that differ from the winning rule) block auto-apply for that stream identity.
2. All matching authored rules and persisted stream rules are collected as candidates.
3. Highest `priority` wins; ties break by candidate order.
4. Authored rules typically use positive priority (default `10`). Dashboard-saved `routing_rules` use implicit low priority (`-1000` minus index) so authored rules win when both match.

On first launch after upgrading from Phase 2-only configs, existing `routing_rules.stream_rules` are migrated into `rules[]` once (when `rules` is empty) and cleared from `routing_rules`.

### Lightweight persisted routes (`routing_rules`, Phase 2+)

When a user picks a route from the dashboard matrix, Pipe Deck also saves a lightweight rule so the choice survives idle streams and re-applies when audio starts again. These coexist with authored `rules[]` at lower priority.

```yaml
routing_rules:
  stream_rules:
    - app_name: Firefox
      executable: firefox
      target_system_name: pipe-deck-test
    - app_name: Soundux
      media_name: miniaudio
      target_system_name: soundux_sink
  device_rules:
    - source_system_name: soundux_sink
      target_system_name: pipe-deck-test
      safeguards:
        fallback_policy: keep_current
```

#### Semantics

- **stream_rules:** When a matching playback/capture stream appears, move it to `target_system_name`.
- **device_rules:** Link virtual sink monitor ports to the target device (`pw-link` for sink→output or sink→virtual mic). Like authored stream rules, each device rule carries a `safeguards.fallback_policy` (default `keep_current`): if the configured target device is missing from the graph, `keep_current` leaves the virtual sink unrouted (prior behavior), while `safe_default` reroutes it to the first available physical output. There's no UI to set this yet — dashboard-picked device routes always save with the default `keep_current` policy.
- Stream rules are replaced per composite identity key (`app_name` + optional `executable` + optional `media_name`), not per `app_name` alone.
- Dashboard dropdown + connection lines show live state; the Rules view lists authored policies; explainability panels show why each stream routed.

## Profile Swap Semantics

1. **Load:** core reads profile YAML from disk.
2. **Validate:** schema version, required fields, routing intent shape.
3. **Restore virtual devices:** recreate profile `device_assumptions` if missing from PipeWire.
4. **Apply:** core sends routing intents to PipeWire integration layer.
5. **Commit:** on success, update `active_profile` in `config.yaml`; emit state events; UI re-renders.
6. **Rollback:** on failure, revert to last known-good applied state; surface actionable error.

Profile swap must be atomic from the user's perspective: either the new profile is fully applied or the prior state is restored.

## Save Profile Flow

1. User requests save (or save-as).
2. Core captures current routing state from runtime graph.
3. Core writes new or updated YAML to `profiles/<name>.yaml`.
4. Core updates profile index in `config.yaml` if new profile.

## Export and Import

- **Export:** copy one or more profile files, or bundle into a `.tar.gz` / `.zip` archive with manifest.
- **Import:** place profile file in `profiles/` directory; core validates and adds to index.
- No separate database export step; files are the source of truth.
- Import on another machine: copy profile file(s) into config directory; swap to activate.

## Decisions

- Profiles are stored as separate YAML files by default (PD-003).
- Main config maintains a lightweight profile index and active profile pointer.
- SQLite is deferred; YAML remains the portable contract for future migration if needed.
- Export/import is file copy or simple archive, not a proprietary binary format.

## Defaults and Override Behavior

- Ship minimal defaults that work on common setups.
- User-defined values always override defaults.
- Unknown keys should be preserved when possible to avoid data loss.

## Migration Strategy

- Schema version required in each saved config and profile.
- Provide deterministic version-to-version migration steps.
- Keep migration logs user-readable for transparency.
- Future SQLite store, if introduced, must import/export the same YAML schema.

## Validation Requirements

- Reject malformed config with actionable errors.
- Warn on unknown high-risk fields.
- Fallback to last valid state when load or apply fails.
- Profile apply failure triggers rollback, not partial application.

## Traceability to User Value

- Predictable config behavior → fewer broken setups after updates.
- Explicit profile structure → easier backup, share, and recovery.
- File-first model → users can inspect, edit, and copy setups without tooling.
