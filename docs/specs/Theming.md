# Theming

## Purpose

Document how Pipe Deck's color schemes work: the built-in schemes shipped with the app, how Settings → General → Appearance resolves which scheme is active, and the YAML schema power users write to define their own.

## Built-in schemes

| id | name | kind |
|---|---|---|
| `midnight-deck` | Midnight Deck | dark |
| `copper-dusk` | Copper Dusk | dark |
| `paper-deck` | Paper Deck | light |
| `meadow-light` | Meadow Light | light |

`midnight-deck` and `paper-deck` are the fallback base palettes custom schemes resolve against (see below).

## Mode resolution

Settings → General → Appearance has a **Mode** selector (Light / Dark / Follow system) plus independent **Dark scheme** and **Light scheme** pickers. The active scheme is resolved as:

1. Mode = Light → the selected **Light scheme**.
2. Mode = Dark → the selected **Dark scheme**.
3. Mode = Follow system → whichever of the two matches the OS's `prefers-color-scheme`, switching live if the OS theme changes while the app is open.

This lets you pair any dark scheme with any light scheme — e.g. Copper Dusk for dark and Meadow Light for light — independently of which mode is currently active.

## Custom schemes

Drop one YAML file per scheme in:

```
~/.config/pipe-deck/themes/*.yaml
```

(respects the `PIPE_DECK_CONFIG_DIR` override — see [Config Spec](../specs/Config_Spec.md)). Files are auto-discovered; the picker in Settings shows every valid file in that directory alongside the 4 built-ins.

### Schema

```yaml
name: My Custom Theme      # required — this is what shows in the Settings picker,
                            # not the filename
base: dark                 # required — "light" or "dark"; any color key you don't
                            # set below falls back to that base's built-in default
                            # (midnight-deck for dark, paper-deck for light)
colors:                    # optional — set only the keys you want to override
  background: "#0b0f14"
  surface_1: "#131820"
  surface_2: "#1c2330"
  border: "#2a3344"
  text: "#e6e9ef"
  text_muted: "#9aa4b2"
  accent_purple: "#7c5cff"
  accent_teal: "#26c3a3"
  accent_amber: "#ffb020"
  status_success: "#34d399"
  status_warning: "#fb923c"
  status_danger: "#f87171"
```

All 12 `colors` keys are optional and independent — set as many or as few as you like.

### What each key controls

This is the complete set of overridable keys — every color used anywhere in the app resolves to one of these 12 tokens (`src/styles/_variables.scss` is the canonical list; nothing in the UI is styled with a color outside this set).

| Key | Used for |
|---|---|
| `background` | Page background: sidebar, top bar, main content area, footer |
| `surface_1` | Primary card/panel surfaces (dashboard cards, mixer strips, modal bodies) |
| `surface_2` | Secondary/recessed surfaces (inputs, pills, nested rows, hover states) |
| `border` | All hairline borders and dividers |
| `text` | Primary text and icon color |
| `text_muted` | Secondary/label text, hints, timestamps |
| `accent_purple` | Primary interactive accent (active nav, focus rings, primary buttons) |
| `accent_teal` | Secondary accent (capture/input-side UI, links) |
| `accent_amber` | Tertiary accent (output/warning-adjacent UI, mute indicators) |
| `status_success` | "OK"/connected/up-to-date indicators |
| `status_warning` | "Outdated"/degraded indicators |
| `status_danger` | Error states, destructive actions, disconnected indicators |

If you find UI that doesn't visibly change when you set one of these keys, that's a bug — file an issue rather than assuming it's an unlisted token; nothing should bypass this set.

### Worked example: override just two accents

```yaml
name: Neon Night
base: dark
colors:
  accent_purple: "#ff2d95"
  accent_teal: "#00e5ff"
```

This inherits every other Midnight Deck color (background, surfaces, border, text) unchanged, and only recolors the purple and teal accents used throughout the UI.

### Validation and error handling

- A file with an unknown/misspelled `base` value, or that otherwise fails to parse, is **skipped** — it won't appear in the picker, but it doesn't break the rest of your custom schemes or crash the app.
- If a scheme you have selected in Settings is later removed (file deleted), the app falls back to the built-in default for that mode (`midnight-deck` for dark, `paper-deck` for light).

## Known limits

- **Native window chrome (title bar, minimize/maximize/close controls):** Pipe Deck uses the OS's native window decorations rather than drawing its own, so these aren't styled by the 12 tokens above. The app does hint the OS to render them dark or light via Tauri's cross-platform `Window.setTheme()` API whenever the resolved scheme's `kind` changes — this affects the title bar on Windows and macOS, and (where the desktop environment respects the hint) Linux GTK client-side decorations. It cannot recolor them to match a specific custom scheme's palette, only nudge them dark vs. light, and some window managers/DEs may not honor the hint at all. There is no per-scheme control here, and none is planned — it's inherently OS/DE-dependent.

## Architecture notes

- Colors are applied at runtime as CSS custom properties (`--surface-1`, `--text`, `--accent-purple`, etc.) set on the document root, not baked into the SCSS build. See [Decisions](../architecture/Decisions.md) (PD-018) for the rationale.
- The merge of a custom scheme's partial overrides against its base palette happens in Rust (`src-tauri/src/config/theme_store.rs`) — the frontend and any future plugin consumer only ever see a fully-resolved 12-color palette, never a partial one.
- SCSS partials never hardcode a color that should track the theme — they reference these 12 CSS custom properties, plus two small helpers in `src/styles/_variables.scss` to avoid repeating verbose syntax: the `translucent($token, $percent)` Sass function for alpha-blended tints (e.g. `vars.translucent(--background, 95)`), and the `%inset-highlight` placeholder for the repeated card-surface highlight. If you're adding new UI and reach for a raw hex or `rgba(...)`, check this table first — it's very likely one of these 12 keys already covers it.

## Future: plugin access to themes (not yet implemented)

This is deliberately out of scope for the initial theming work: a future `ui.theme.read` capability is planned for [Plugin API](../specs/Plugin_API.md) so that `ui.panel.register` plugins can read the active resolved palette and match host styling. No plugin capability currently exposes theme data.
