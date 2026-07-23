# Plugin API

## Purpose

Define the extension contract for Pipe Deck plugins: transport, capabilities, lifecycle, and security boundaries.

## API Version

- **Current:** `1`
- Plugins declare `api_version: 1` in their manifest; the host rejects incompatible versions.

## Transport

- **Protocol:** JSON-RPC 2.0 over stdin/stdout (newline-delimited messages).
- **Process model:** Each plugin runs in an isolated subprocess (PD-004).
- **Timeouts:** Host requests time out after 5 seconds; hung plugins are killed without affecting core routing.

### Host → Plugin methods

| Method | Description |
|--------|-------------|
| `initialize` | Handshake with granted capabilities and config directory path |
| `shutdown` | Clean shutdown before process termination |
| `graph.updated` | Push throttled runtime graph snapshot (notification, no id) |
| `profile.updated` | Push active profile metadata (`id`, `name`, `updated`) on start and on every profile swap (notification, no id) — gated by `profile.read` |

### Plugin → Host methods (capability-gated)

| Method | Required capability | Implemented |
|--------|-------------------|--------------|
| `ui.panel.register` | `ui.panel.register` | Yes |
| `routing.suggest` | `routing.suggest` | Yes — notification with `{stream_id, target_system_name, reason?}`; the host stores it (no apply) and audit-logs it |
| `effects.apply` | `effects.manage` | Not yet — capability is declarable/grantable but the host has no handler for this method today |

### Example handshake

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"api_version":1,"plugin_id":"pipe-deck-effects","granted_capabilities":["graph.read","effects.manage","ui.panel.register"],"config_dir":"/home/user/.config/pipe-deck"}}
```

```json
{"jsonrpc":"2.0","id":1,"result":{"plugin_version":"0.1.0","status":"ready"}}
```

## Manifest Schema

Each plugin ships a `plugin.yaml` beside its entry binary:

```yaml
id: pipe-deck-effects
name: Pipe Deck Effects
version: 0.1.0
api_version: 1
entry: bin/pipe-deck-effects
capabilities:
  - graph.read
  - effects.manage
  - ui.panel.register
description: First-party EQ and compressor for Pipe Deck virtual devices
bundled: true
```

### Fields

| Field | Required | Description |
|-------|----------|-------------|
| `id` | yes | Stable identifier (lowercase, hyphen-separated) |
| `name` | yes | User-facing display name |
| `version` | yes | Semver plugin version |
| `api_version` | yes | Host API version this plugin targets |
| `entry` | yes | Relative path to executable from plugin root |
| `capabilities` | yes | Requested capabilities (denied until user approves) |
| `description` | no | Short summary for Settings UI |
| `bundled` | no | `true` for first-party plugins shipped with the app |
| `developer` | no | Author/org name, shown in the Plugins table and detail view |
| `repo` | no | Source repository URL, shown in the Plugins table and detail view |

## Discovery Paths

| Location | Purpose |
|----------|---------|
| `$RESOURCE/plugins/<id>/` | Bundled first-party plugins |
| `~/.config/pipe-deck/plugins/<id>/` | User-installed plugins |

## Capabilities (v1)

Capabilities are explicit, reviewable, and revocable in Settings.

| Capability | Access | Enforced today |
|------------|--------|-----------------|
| `graph.read` | Receive `graph.updated` notifications with runtime graph JSON | Yes |
| `routing.suggest` | Send route suggestions to the host (no apply) | Yes |
| `profile.read` | Receive `profile.updated` notifications with active profile metadata | Yes |
| `effects.manage` | Create/update PipeWire filter-chain on `pipe-deck-*` devices only | Yes — via a queued-request model (PD-021): a plugin sends an `effects.apply` notification, and the host applies it through the existing `set_device_effects` path on the next graph-refresh tick |
| `ui.panel.register` | Register a nav panel (id, title, summary HTML) rendered by host UI | Yes |

All five v1 capabilities are enforced as of PD-021 (`docs/architecture/Decisions.md`).

**Out of v1:** `routing.apply`, `profile.write` — require a future API revision with explicit approval and audit.

## Lifecycle

1. **Discover** — scan bundled and user plugin directories for `plugin.yaml`.
2. **Validate** — check manifest schema, `api_version`, entry binary exists.
3. **Authorize** — compare requested vs user-granted capabilities in `config.yaml`.
4. **Initialize** — spawn subprocess, send `initialize` RPC, wait for ready response.
5. **Run** — push graph events; handle plugin RPCs through capability gate.
6. **Shutdown** — `shutdown` RPC → SIGTERM → reap; audit log entry.

## Security

- Principle of least privilege; deny-by-default until user approves in Settings.
- Plugin failures must not crash or block core routing (PD-004).
- Audit log: `~/.local/state/pipe-deck/plugin-audit.jsonl` (timestamp, plugin_id, action, result).
- Plugins cannot mutate PipeWire directly; scoped operations go through the host.

## Versioning

- Semantic versioning for plugin releases.
- Host maintains a compatibility matrix; breaking API changes increment `api_version`.
- Deprecation windows documented before capability removal.

## Extension Boundaries

**Allowed:** rule suggestions, profile translators, device labeling, UI panels, first-party effects.

**Restricted (core-owned):** unrestricted PipeWire mutation, safety policy bypass, privileged background ops without approval.

## Related

- `docs/specs/Config_Spec.md` — `plugins:` config block
- `docs/developers/Plugins.md` — contributor guide
- `docs/architecture/Decisions.md` — PD-004, PD-014, PD-015, PD-016
