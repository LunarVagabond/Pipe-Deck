# Soundboard Spec

## Purpose

Define how Pipe Deck lets a user trigger short audio clips (sound effects, drops, alerts, voice clips) that play into a chosen device — a virtual input/mic or a hardware input passthrough — on demand, without leaving the app or relying on a separate third-party soundboard tool.

## In Scope

- Backend playback primitive: playing an arbitrary audio file into a named target device.
- Configured clip source (a folder of sound files) and per-clip target-device assignment.
- Soundboard tab UI: browsing configured clips and triggering playback.

## Out of Scope (for now)

- Per-clip volume or effects-chain routing — a clip plays at its own recorded level straight into the target device.
- Global hotkeys to trigger clips while the app is backgrounded (tracked separately, issue #128).
- Stop/interrupt of an in-progress clip (tracked separately, issue #399 — deferred until the playback primitive itself has shipped and its shape is known).

## Delivery

Tracked as epic issue #127 with native GitHub sub-issues, built in this order:

1. **#393 — backend playback primitive.** `AudioBackend::play_sound(path, target_system_name)`, implemented via `pw-cat --playback --target <target_system_name>`. Fire-and-forget: returns once playback has started, not once it finishes, with no stop handle. See `docs/architecture/Decisions.md` PD-036 for the full design rationale. `CoreEngine::play_sound(path, target_device_id)` (`core/engine/soundboard_ops.rs`) is the thin engine-level wrapper other tickets build on, resolving a domain device id to the `system_name` the backend call needs.
2. **#396 — Soundboard tab UI skeleton.** New sidebar view, built against a static empty state first (no backend wiring yet).
3. **#394 — configured folder + file listing.** `Preferences.soundboard_folder` (`config.yaml`), `core::soundboard::list_sounds` (non-recursive, extension-filtered: wav/flac/ogg/oga/mp3/aiff/aif/m4a/opus), and the `get_soundboard_folder`/`set_soundboard_folder`/`list_soundboard_sounds` Tauri commands. Wired into the Soundboard tab from #396: a folder-path field plus a Soundux-style grid of tiles, one per listed clip. Tiles are inert — no play affordance yet, that's #397.
4. **#395 — per-sound target-device mapping.** Persists which device each clip plays through. Target-device selection lives in the Soundboard tab itself, not Settings — it's specific to this feature, not a general preference (the interaction model draws on how Soundux handles per-sound target selection).
5. **#397 — play button wiring.** Connects the tab's play action to the backend primitive and the per-sound target mapping.
6. **#398 — target-device picker UI.** The in-tab control backing #395's mapping.
7. **#399 — stop/interrupt playback.** Lower priority; scoped once the playback primitive's real shape (process handle, lifetime) is established.

## Related Documents

- `docs/architecture/Decisions.md` (PD-036)
- `docs/specs/Config_Spec.md`
- `docs/specs/UI_Spec.md`
