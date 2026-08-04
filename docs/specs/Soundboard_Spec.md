# Soundboard Spec

## Purpose

Define how Pipe Deck lets a user trigger short audio clips (sound effects, drops, alerts, voice clips) that play into a chosen device — a virtual input/mic or a hardware input passthrough — on demand, without leaving the app or relying on a separate third-party soundboard tool.

## In Scope

- Backend playback primitive: playing an arbitrary audio file into a named target device.
- Configured clip sources: user-named tabs (Soundux-style, e.g. "SFX", "Music"), each backed by its own folder of sound files, and per-clip target-device assignment.
- Soundboard tab UI: browsing configured clips and triggering playback.

## Out of Scope (for now)

- Per-clip volume or effects-chain routing — a clip plays at its own recorded level straight into the target device.
- Global hotkeys to trigger clips while the app is backgrounded (tracked separately, issue #128).
- Stop/interrupt of an in-progress clip (tracked separately, issue #399 — deferred until the playback primitive itself has shipped and its shape is known).

## Delivery

Tracked as epic issue #127 with native GitHub sub-issues, built in this order:

1. **#393 — backend playback primitive.** `AudioBackend::play_sound(path, target_system_name)`, implemented via `pw-cat --playback --target <target_system_name>`. Fire-and-forget: returns once playback has started, not once it finishes, with no stop handle. See `docs/architecture/Decisions.md` PD-036 for the full design rationale. `CoreEngine::play_sound(path, target_device_id)` (`core/engine/soundboard_ops.rs`) is the thin engine-level wrapper other tickets build on, resolving a domain device id to the `system_name` the backend call needs.
2. **#396 — Soundboard tab UI skeleton.** New sidebar view, built against a static empty state first (no backend wiring yet).
3. **#394 — configured folders + file listing, as named tabs.** `Preferences.soundboard_boards: Vec<SoundboardBoard>` (`config.yaml`) — each board is `{ id, name, folder }`, id minted client-side (`crypto.randomUUID()`, same convention as `Rule.id`). `core::soundboard::list_sounds` lists a single folder's clips (non-recursive, extension-filtered: wav/flac/ogg/oga/mp3/aiff/aif/m4a/opus); `list_soundboard_boards`/`save_soundboard_board`/`delete_soundboard_board`/`list_soundboard_sounds(board_id)` are the Tauri commands. Wired into the Soundboard tab from #396: a tab bar (`SegmentedControl`, one option per board plus a "+" to add) switches between boards, each showing its own folder path, a native OS folder picker (`@tauri-apps/plugin-dialog`) to set/change it, and a Soundux-style grid of tiles for its clips. Tiles are inert — no play affordance yet, that's #397.
4. **#395 — per-sound target-device mapping.** `SoundboardBoard.clip_targets: HashMap<String, String>` — clip id (file name) → target device `system_name`, persisted alongside the board. Keyed by `system_name`, not a domain device id, matching every other persisted device reference in `config.yaml` (`StreamRouteRule.target_system_name`, `MixSourceSpec.source_system_name`) — a domain id is only stable for one session. No UI yet; updates go through the existing `save_soundboard_board` upsert (no new command). Target-device selection lives in the Soundboard tab itself, not Settings — it's specific to this feature, not a general preference (the interaction model draws on how Soundux handles per-sound target selection).
5. **#397 — play button wiring.** `CoreEngine::play_soundboard_clip(board_id, clip_id)` (`core/engine/soundboard_ops.rs`) re-resolves the clip fresh from config + disk on every call (loads the board, re-lists its folder rather than trusting a client-supplied path, looks up the persisted target) and plays it via the #393 primitive — errors clearly if the board/clip doesn't exist or no target is assigned yet. The Tauri command (`commands::soundboard::play_soundboard_clip`) is a one-line wrapper; all the logic lives in the engine per this codebase's thin-commands convention. Each Soundboard tile is now a real button: clicking it plays the clip, and a tile with no target assigned is visually dimmed (`.no-target`) using the board's own `clip_targets` the frontend already has client-side — no extra round trip needed to know which clips are ready to play.
6. **#398 — target-device picker UI.** The in-tab control backing #395's mapping.
7. **#399 — stop/interrupt playback.** Lower priority; scoped once the playback primitive's real shape (process handle, lifetime) is established.

## Related Documents

- `docs/architecture/Decisions.md` (PD-036)
- `docs/specs/Config_Spec.md`
- `docs/specs/UI_Spec.md`
