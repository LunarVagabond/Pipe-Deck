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
  landing_view: dashboard
  apply_immediately: true
active_profile: gaming
profile_index:
  - id: gaming
    name: Gaming
    file: profiles/gaming.yaml
  - id: streaming
    name: Streaming
    file: profiles/streaming.yaml
devices: {}        # known device metadata and aliases
rules: []          # rule definitions (Phase 3+)
plugins: {}        # plugin enablement (Phase 5+)
diagnostics:
  verbosity: normal
```

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
- **Device assumptions:** optional hints for restore (e.g., expected USB interface)
- **Rule overrides:** optional rule activation overrides (Phase 3+)

## Profile Swap Semantics

1. **Load:** core reads profile YAML from disk.
2. **Validate:** schema version, required fields, routing intent shape.
3. **Apply:** core sends routing intents to PipeWire integration layer.
4. **Commit:** on success, update `active_profile` in `config.yaml`; emit state events; UI re-renders.
5. **Rollback:** on failure, revert to last known-good applied state; surface actionable error.

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
