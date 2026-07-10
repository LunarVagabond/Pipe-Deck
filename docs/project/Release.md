# Releasing Pipe Deck

Pipe Deck ships from a **single repo**: [LunarVagabond/Pipe-Deck](https://github.com/LunarVagabond/Pipe-Deck). GitHub Releases hosts install bundles and `latest.json` for in-app update checks.

## One-time setup: updater signing

Release builds sign the AppImage for Tauri’s in-app updater. The **public** key is committed in `src-tauri/tauri.conf.json`. The **private** key must stay secret.

### Generate keys (if you have not already)

```bash
mkdir -p .tauri
npm run tauri signer generate -- --ci -w .tauri/pipe-deck.key -p "" -f
```

Copy the public key from `.tauri/pipe-deck.key.pub` into `src-tauri/tauri.conf.json` → `plugins.updater.pubkey` if you regenerate keys.

### GitHub Actions secrets

In the repo **Settings → Secrets and variables → Actions**, add:

| Secret | Value |
|--------|--------|
| `TAURI_SIGNING_PRIVATE_KEY` | Full contents of `.tauri/pipe-deck.key` (one base64 line) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Leave empty, or your key password if you set one |

Without these secrets, the release job fails when the AppImage `.sig` is missing.

---

## Cut a release (local)

From `main`, with a clean working tree:

```bash
# Bump versions, commit, and create an annotated tag
make release VER=0.2.0
# Optional title slug: make release VER=0.2.0 TITLE="First public beta"
#   → tag v0.2.0-first-public-beta

git push origin main --tags
```

`make release` updates:

- `package.json` / `package-lock.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`
- `packaging/com.pipedeck.PipeDeck.metainfo.xml`

Then creates commit `Release <tag>` (if needed) and annotated tag `<tag>`.

---

## What CI does

Workflow: [`.github/workflows/build.yml`](../.github/workflows/build.yml)

| Event | Jobs |
|-------|------|
| PR or push to `main` | `check` + `smoke` |
| Push tag `v*` | `release` — build Linux bundles, stage assets, **draft GitHub Release** |

Release assets:

- `.AppImage` + `.sig` (in-app auto-update for AppImage installs)
- `.deb`, `.rpm`
- `pipe-deck-<tag>-linux-x86_64.tar.gz` (standalone `pipe-deck` + `pipe-deck-daemon`)
- `latest.json` — multi-format update manifest

Updater endpoint (same repo):

```text
https://github.com/LunarVagabond/Pipe-Deck/releases/latest/download/latest.json
```

---

## Publish the release

1. Wait for the **Release** job on the tag push to finish.
2. Open [GitHub Releases](https://github.com/LunarVagabond/Pipe-Deck/releases).
3. Open the **draft** for your tag.
4. Edit title/notes if needed.
5. Click **Publish release**.

Until the draft is published, the app’s update check may not see the new version (`/releases/latest` points at the latest **published** release).

---

## In-app updates

Settings → **About** → **Check for updates** reads `latest.json` and picks the package for how Pipe Deck was installed:

| Install type | Update behavior |
|--------------|-----------------|
| AppImage | In-app install via Tauri updater |
| `.deb` / `.rpm` / binary | Opens the matching download URL |
| Flatpak | Opens flatpak entry when present in manifest |
| Dev build (commit hash) | Update check unavailable |

---

## Tag format

- `v0.2.0` — version `0.2.0`
- `v0.2.0-hotfix-title` — version `0.2.0`, slug for release notes only

CI parses semver as the first `X.Y.Z` after the leading `v`.

---

## Troubleshooting

**Release job: version mismatch** — run `make release` locally; do not hand-tag without bumping version files.

**Missing `.sig`** — set `TAURI_SIGNING_PRIVATE_KEY` (and password if used) in GitHub secrets.

**Updater does not find a package** — confirm the draft is **published** and `latest.json` is attached to that release.

**Regenerated signing keys** — update `plugins.updater.pubkey` in `tauri.conf.json`, GitHub secret, and ship a new release; older builds cannot verify signatures from a new keypair.
