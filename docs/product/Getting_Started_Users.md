# Getting Started with Pipe Deck

Pipe Deck exists so audio routing on Linux doesn't require being a PipeWire expert. This guide gets you from install to your first working route in a few minutes — no config files, no `pw-cli`.

Setting up to contribute code instead? See [Getting Started (from source)](../developers/Getting_Started.md).

## Install

Grab the latest release from [GitHub Releases](https://github.com/LunarVagabond/Pipe-Deck/releases/latest):

- **AppImage** — download, mark executable, run. No install step, works on almost any distro.
- **.deb** — for Debian, Ubuntu, Pop!_OS, Mint.
- **.rpm** — for Fedora and other RPM-based distros.

## First launch

Open Pipe Deck and you land on the **Dashboard** — a live overview of every device and stream PipeWire currently knows about: playback apps, capture apps, outputs in use, and any virtual devices you've created.

![Dashboard — live audio graph](../images/dashboard.png)

## Create your first route

Open **Routing** in the sidebar. Every app producing or capturing audio shows up as a node on the left; outputs like your headphones or speakers sit on the right. Drag from an open output slot on one node to an open input slot on another to connect them — audio starts flowing immediately, no apply/confirm step.

![Routing — application to output](../images/routing.png)

## Connect an app (e.g. OBS)

Launch the app you want to route — say, OBS, Discord, or Spotify. It shows up as a new stream node in Routing (and in **Sources**, if it's a capture app like OBS recording your mic). Drag its output to whatever sink makes sense: your headphones directly, or a virtual sink like "Stream Mix" if you want a separate mix for streaming without touching your main output.

![Sources — capture streams and inputs](../images/sources.png)

## Done — what's next

- **Profiles** — save this setup as a named profile and restore it after a reboot instead of rebuilding it by hand.
- **Rules** — automate routing so specific apps always land where you want, with simulation before anything applies.
- Browse the rest of the docs from the [documentation map](../README.md).
