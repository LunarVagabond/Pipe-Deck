# Soundboard Spec

## Purpose

Define how Pipe Deck lets a user trigger short audio clips (sound effects, drops, alerts, voice clips) that play into a chosen device — a virtual input/mic or a hardware input passthrough — on demand, without leaving the app or relying on a separate third-party soundboard tool.

## In Scope

- Backend playback primitive: playing an arbitrary audio file into a named target device.
- Configured clip sources: user-named tabs (Soundux-style, e.g. "SFX", "Music"), each backed by its own folder of sound files, and per-**tab** target/monitor device assignment with independent volumes.
- Soundboard tab UI: browsing configured clips and triggering playback.
- Stop/interrupt of an in-progress clip, with a Spotify-style progress bar (elapsed on the left, remaining time on the right) on each currently playing tile.
- Per-tab playback policy: exclusive mode stops the current clip before a new one starts; overlap mode lets clips play concurrently.

## Out of Scope (for now)

- Per-clip volume or effects-chain routing — a clip plays at its own recorded level straight into the target device.
- Global hotkeys to trigger clips while the app is backgrounded (tracked separately, issue #128).

## Delivery

Tracked as epic issue #127 with native GitHub sub-issues, built in this order:

1. **#393 — backend playback primitive.** `AudioBackend::play_sound(path, target_system_name)`, implemented via `pw-cat --playback --target <target_system_name>`. Fire-and-forget: returns once playback has started, not once it finishes, with no stop handle. See `docs/architecture/Decisions.md` PD-036 for the full design rationale. `CoreEngine::play_sound(path, target_device_id)` (`core/engine/soundboard_ops.rs`) is the thin engine-level wrapper other tickets build on, resolving a domain device id to the `system_name` the backend call needs.
2. **#396 — Soundboard tab UI skeleton.** New sidebar view, built against a static empty state first (no backend wiring yet).
3. **#394 — configured folders + file listing, as named tabs.** `Preferences.soundboard_boards: Vec<SoundboardBoard>` (`config.yaml`) — each board is `{ id, name, folder }`, id minted client-side (`crypto.randomUUID()`, same convention as `Rule.id`). `core::soundboard::list_sounds` lists a single folder's clips (non-recursive, extension-filtered: wav/flac/ogg/oga/mp3/aiff/aif/m4a/opus); `list_soundboard_boards`/`save_soundboard_board`/`delete_soundboard_board`/`list_soundboard_sounds(board_id)` are the Tauri commands. Wired into the Soundboard tab from #396: a tab bar (`SegmentedControl`, one option per board plus a "+" to add) switches between boards, each showing its own folder path, a native OS folder picker (`@tauri-apps/plugin-dialog`) to set/change it, and a Soundux-style grid of tiles for its clips. Tiles are inert — no play affordance yet, that's #397.
4. **#395 — target-device persistence (superseded in shape by #398, see below).** Originally shipped as a per-clip `SoundboardBoard.clip_targets` map; #398 replaced this with board-wide fields once the actual design was confirmed. `system_name`-keyed persistence (not a domain device id) was the one part of this ticket that stuck — matches every other persisted device reference in `config.yaml` (`StreamRouteRule.target_system_name`, `MixSourceSpec.source_system_name`), since a domain id is only stable for one session.
5. **#397 — play button wiring.** `CoreEngine::play_soundboard_clip(board_id, clip_id)` (`core/engine/soundboard_ops.rs`) re-resolves the clip fresh from config + disk on every call (loads the board, re-lists its folder rather than trusting a client-supplied path) and plays it via the #393 primitive. Each Soundboard tile is a real button: clicking it plays the clip. The Tauri command (`commands::soundboard::play_soundboard_clip`) is a one-line wrapper; all the logic lives in the engine per this codebase's thin-commands convention.
6. **#398 — target/monitor device + volume, per tab.** `SoundboardBoard` grew four flat fields — `target_system_name`, `target_volume_percent`, `monitor_system_name`, `monitor_volume_percent` — replacing #395's per-clip `clip_targets` map outright (see `docs/architecture/Decisions.md` PD-038). Every clip in a tab plays through the same two destinations: `target` (what other people/apps hear, e.g. a virtual mic) and `monitor` (a local output, e.g. the user's own speakers, so they can hear/test a clip independent of what the target gets), each with its own volume — `AudioBackend::play_sound` gained a `volume_percent` parameter (applied via `pw-cat --volume`) to support this. The controls live in the Soundboard tab itself (two device dropdowns + two range sliders, next to Refresh/Change folder/Rename/Delete tab), not Settings. Per-clip overrides were deliberately cut after initial review; per-tab (board-wide) is the ceiling for now — finer-grained mapping is a possible future ticket, not built speculatively here.
7. **#399 — stop/interrupt playback + progress bar.** `AudioBackend::stop_sound()` (default no-op, overridden by `LinuxPipeWireBackend` and `MockAudioBackend`) kills whatever `pw-cat` child process(es) `LinuxPipeWireBackend` is tracking from the most recent `play_sound` call(s) — up to two, one per leg (#398). `core::soundboard::list_sounds` now also probes each clip's `duration_seconds` via `lofty` (file header/metadata, no decoding); the frontend uses that plus a client-side interval timer to render the progress bar (Spotify-style: elapsed on the left counting up from 0:00, the clip's static length on the right) with no backend completion event needed — see `docs/architecture/Decisions.md` PD-039.
8. **#408 — per-tab overlap policy.** `SoundboardBoard.exclusive_playback` is persisted per tab and defaults to `true` when absent, retaining the post-#399 single-clip behavior for existing configurations. The active tab's **Exclusive playback** toggle saves through the existing board persistence path. When disabled, a second tile click goes directly to `play_soundboard_clip` and leaves existing client playback state visible; when enabled, the frontend requests the existing `stop_soundboard_clip` command and starts the new clip only after that stop succeeds. The stop command remains global for the backend's tracked `pw-cat` legs, so per-clip stop semantics are not implied by this toggle.

## Related Documents

- `docs/architecture/Decisions.md` (PD-036, PD-039, PD-053)
- `docs/specs/Config_Spec.md`
- `docs/specs/UI_Spec.md`
