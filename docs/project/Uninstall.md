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

The one-shot way: **before** removing the package, run

```bash
pipe-deck-cli cleanup --purge-config
```

This unloads every live Pipe Deck `pactl` module, disables and removes the background-restore systemd unit, deletes any live-effects drop-ins, and (with `--purge-config`) removes `~/.config/pipe-deck` and the daemon state directory. It prints a JSON summary of what it did. Run it without `--purge-config` first if you just want the *running session* clean but plan to reinstall and keep your profiles/config. This isn't wired into `apt remove`/`dnf remove` automatically — see "Why this isn't automatic" below — so it has to be run explicitly, before or in place of the package removal step.

The manual equivalent, if you'd rather not use the CLI or the binary is already gone:

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
5. Unload any still-live virtual devices from the current PipeWire session (step 3 only stops them being *recreated* on next launch — a module already loaded stays loaded in the running session until it's unloaded or PipeWire restarts):
   ```bash
   pactl list short modules | grep pipe-deck | cut -f1 | xargs -r -n1 pactl unload-module
   ```
   Or just restart PipeWire (see "Resetting PipeWire itself back to stock" below) if a brief audio interruption is acceptable.

## Why this isn't automatic on `apt remove`/`dnf remove`

Tauri's bundler (what builds Pipe Deck's `.deb`/`.rpm`) has no `postrm`/`postinst`/`%postun` hook mechanism today — there's no config surface to ship a cleanup script that runs automatically on package removal. `pipe-deck-cli cleanup` exists so there's at least an explicit, scriptable one-shot instead of nothing; a distro packager building their own `.deb`/`.rpm` outside this repo's bundler config is free to wire it into a `prerm` script (it must run in `prerm`, before files are deleted — by the time `postrm` runs, `pipe-deck-cli` itself is already gone).

## Flatpak

The `flatpak/` manifest is kept in the repo for a future contributor (see `flatpak/README.md`) but isn't built by CI or documented as a supported install path (dropped in #201/#203). If you built and installed it yourself: Flatpak's sandbox means `pipe-deck-cli cleanup` and the systemd-unit steps above generally can't reach your real user session (`~/.config/systemd/user`, `pactl`) from inside the sandbox at all — `flatpak uninstall` removes the sandboxed app and its isolated data, but anything Pipe Deck reached outside the sandbox (if the manifest grants that access) would need the same manual `pactl unload-module`/systemd steps above, run from a normal (non-sandboxed) shell.

## Resetting PipeWire itself back to stock

If you want to confirm your audio stack has no Pipe Deck state left at all, beyond the app's own files:

```bash
pactl list short modules | grep -i pipe-deck   # should be empty once virtual devices are gone
systemctl --user restart pipewire pipewire-pulse wireplumber
```

Pipe Deck never installs or modifies `filter-chain.service` itself — that unit ships with your PipeWire package and exists whether or not Pipe Deck is installed. Restarting it (or the core `pipewire`/`wireplumber` units above) only reloads config; it does not uninstall anything.
