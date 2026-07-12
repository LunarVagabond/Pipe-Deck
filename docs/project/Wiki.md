# Wiki Publishing

## Purpose

Describe how the `docs/` tree maps to the GitHub Wiki and what to verify before go-live.

## Wiki layout

When `docs/` becomes the wiki repository (`pipe-deck.wiki`), pages map one-to-one:

| File | Wiki role |
|------|-----------|
| `Home.md` | Default landing page (`/wiki/Home`) |
| `_Sidebar.md` | Custom sidebar on every page |
| `_Footer.md` | Custom footer on every page |
| `product/`, `architecture/`, `specs/`, `project/` | Folder-organized pages |

Open work is tracked in [GitHub Issues](https://github.com/LunarVagabond/Pipe-Deck/issues) on the main repository. The wiki sidebar links to Issues instead of a local backlog file.

## Publishing steps

1. Enable the wiki on the GitHub repository (Settings → Features → Wikis).
2. Clone the empty wiki repo:
   ```bash
   git clone https://github.com/pipedeck/pipe-deck.wiki.git
   ```
3. Copy the contents of `docs/` into the wiki clone (not the `docs/` directory itself — copy inner files to wiki root).
4. Commit and push. Confirm `Home`, sidebar, and footer render on a sample page.
5. Set the wiki **start page** to `Home` if GitHub does not pick it automatically.

## Link conventions

Wiki internal links omit the `.md` extension and use paths relative to the wiki root:

```markdown
[Plugin API](../specs/Plugin_API)     # from project/Plugins.md
[Home](Home)                          # from any page
```

While docs still live in the main repo under `docs/`, GitHub file browsing may require `.md` in links. After wiki migration, use the extensionless form above.

## GitHub repository SEO checklist

Update these in the GitHub UI (not in git):

### About box (repository homepage)

**Description** (≤ 350 chars):

> Open-source Linux audio mixer and PipeWire routing control center — per-app routing, profiles, virtual devices, and automation rules.

**Website** (optional): link to wiki Home or releases page when available.

### Topics

Add repository topics (searchable on GitHub and indexed externally):

```
pipewire
linux-audio
audio-mixer
audio-routing
linux-desktop
tauri
rust
vue
flatpak
open-source
pulseaudio
sound
wayland
```

### Social preview

Upload a 1280×640 image showing the dashboard or mixer (Settings → General → Social preview). Include the tagline: *Linux audio mixer for PipeWire*.

### Wiki settings

- Start page: **Home**
- Confirm `_Sidebar.md` and `_Footer.md` appear after first push

### Issues & discussions

- Pin a **good first issue** or roadmap link for contributors
- Enable **Discussions** if you want a support channel separate from issues

### Releases

Tag releases with descriptive notes mentioning *Linux audio mixer*, *PipeWire*, and install methods (Flatpak, deb, etc.) for search snippets.

## Out of scope

- Automated wiki sync from CI (manual copy is sufficient for now)
- Hosting docs on a separate domain
