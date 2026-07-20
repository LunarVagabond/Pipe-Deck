# Uninstalling & Resetting

## Purpose

Tell you exactly what Pipe Deck leaves on disk and in your PipeWire session, so removing it (or starting over) doesn't mean guessing which files are safe to delete.

## In Scope

- Every file, config drop-in, and systemd unit Pipe Deck creates.
- What removing the app package does and doesn't clean up.
- How to fully reset PipeWire/pactl state back to stock.

## Out of Scope

- Uninstalling PipeWire or WirePlumber themselves — Pipe Deck is a client of your existing audio stack, not a replacement for it (see [About](../product/About.md)).

## The short version

Removing the Pipe Deck package (`apt remove`/`dnf remove`, or deleting the AppImage/binary) only removes the **application** — the binary, desktop file, and icons. It does not touch anything below, because none of it lives alongside the binary; it's all under your user config directories and (optionally) systemd. Nothing Pipe Deck does modifies PipeWire's own system-level config (`/etc/pipewire/`, `pipewire.conf`, WirePlumber config) — it only ever writes its own drop-in file and shells out to `pactl`/`pw-link`, never editing the main PipeWire graph in place.

## What Pipe Deck creates

| What | Where | Created by |
|------|-------|------------|
| Main config (preferences, active profile, routing rules, device aliases) | `~/.config/pipe-deck/config.yaml` | First run |
| Saved profiles | `~/.config/pipe-deck/profiles/*.yaml` | Saving a profile |
| Plugin drop directory | `~/.config/pipe-deck/plugins/` | First run |
| Daemon status file | `~/.local/state/pipe-deck/daemon.json` (or `$XDG_STATE_HOME/pipe-deck/daemon.json`) | Enabling background restore |
| Live effects config | `~/.config/pipewire/filter-chain.conf.d/99-pipe-deck-effects-<device>.conf` — one file per device with effects applied | Applying effects to a device |
| Background-restore systemd unit | `~/.config/systemd/user/pipe-deck-daemon.service` | Enabling "Restore on login" in Settings |
| Virtual devices (sinks/sources) | Not a file — runtime-only `pactl` modules, recreated from `config.yaml`'s `virtual_devices` list each time Pipe Deck starts | Creating a virtual device |

All paths respect `PIPE_DECK_CONFIG_DIR` if you've set it — if you have, use that path instead of `~/.config/pipe-deck/` above.

Pipe Deck writes no log files.

## Full removal

1. Uninstall the package (or delete the binary/AppImage).
2. If you ever turned on **Restore on login** in Settings, disable it first from the app (this stops and disables the systemd unit), or manually:
   ```bash
   systemctl --user disable --now pipe-deck-daemon.service
   rm ~/.config/systemd/user/pipe-deck-daemon.service
   systemctl --user daemon-reload
   ```
3. Remove Pipe Deck's own config and data:
   ```bash
   rm -rf ~/.config/pipe-deck
   rm -rf ~/.local/state/pipe-deck
   ```
4. Remove any live-effects drop-ins:
   ```bash
   rm -f ~/.config/pipewire/filter-chain.conf.d/99-pipe-deck-effects-*.conf
   ```
   If you removed these while Pipe Deck-created virtual devices with effects were still active, restart the shared filter-chain daemon so the change takes effect: `systemctl --user restart filter-chain.service`. (This is a PipeWire-provided unit, not something Pipe Deck installs — see below.)

Virtual devices themselves need no cleanup — they're `pactl` modules that don't persist past a PipeWire/session restart on their own, and step 3 removes the config that would otherwise recreate them.

## Resetting PipeWire itself back to stock

If you want to confirm your audio stack has no Pipe Deck state left at all, beyond the app's own files:

```bash
pactl list short modules | grep -i pipe-deck   # should be empty once virtual devices are gone
systemctl --user restart pipewire pipewire-pulse wireplumber
```

Pipe Deck never installs or modifies `filter-chain.service` itself — that unit ships with your PipeWire package and exists whether or not Pipe Deck is installed. Restarting it (or the core `pipewire`/`wireplumber` units above) only reloads config; it does not uninstall anything.
