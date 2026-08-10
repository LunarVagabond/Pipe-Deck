import { invoke } from "@tauri-apps/api/core";
import { fetch } from "@tauri-apps/plugin-http";
import { check as checkUpdater } from "@tauri-apps/plugin-updater";
import type {
  AppInfo,
  InstallKind,
  UpdateCheckResult,
  UpdateStatus,
} from "../types/app";

export const UPDATE_MANIFEST_URL =
  "https://github.com/LunarVagabond/Pipe-Deck/releases/latest/download/latest.json";
export const RELEASES_PAGE =
  "https://github.com/LunarVagabond/Pipe-Deck/releases/latest";

// The GitHub Releases list API — a different host from the static releases/latest/download
// asset above, so its own allow-list entry is needed in src-tauri/capabilities/default.json.
export const GITHUB_RELEASES_API_URL =
  "https://api.github.com/repos/LunarVagabond/Pipe-Deck/releases";

export type UpdateChannel = "latest" | "prerelease";

export interface UpdatePlatform {
  url: string;
  signature?: string;
}

export interface UpdateManifest {
  version: string;
  notes?: string;
  pub_date?: string;
  platforms: Record<string, UpdatePlatform>;
}

function parseVersion(version: string): [number, number, number] {
  const parts = version
    .replace(/^v/i, "")
    .split(".")
    .map((part) => Number.parseInt(part, 10) || 0);
  return [parts[0] ?? 0, parts[1] ?? 0, parts[2] ?? 0];
}

export function compareUpdateStatus(
  current: string,
  latest: string,
): UpdateStatus {
  const [currentMajor, currentMinor, currentPatch] = parseVersion(current);
  const [latestMajor, latestMinor, latestPatch] = parseVersion(latest);

  const isCurrent =
    latestMajor < currentMajor ||
    (latestMajor === currentMajor && latestMinor < currentMinor) ||
    (latestMajor === currentMajor &&
      latestMinor === currentMinor &&
      latestPatch <= currentPatch);

  if (isCurrent) {
    return "current";
  }

  if (latestMajor > currentMajor || latestMinor > currentMinor + 1) {
    return "severely_outdated";
  }

  return "outdated";
}

export function platformKeyForInstallKind(
  installKind: InstallKind,
): string | null {
  switch (installKind) {
    case "app_image":
      return "linux-x86_64-appimage";
    case "deb":
      return "linux-x86_64-deb";
    case "rpm":
      return "linux-x86_64-rpm";
    case "native":
    case "dev":
      return "linux-x86_64-binary";
    default:
      return null;
  }
}

export async function fetchUpdateManifest(
  manifestUrl: string = UPDATE_MANIFEST_URL,
): Promise<UpdateManifest> {
  const response = await fetch(manifestUrl, {
    headers: { Accept: "application/json" },
  });
  if (!response.ok) {
    throw new Error(`Update manifest fetch failed (${response.status})`);
  }
  return (await response.json()) as UpdateManifest;
}

interface GithubReleaseSummary {
  tag_name: string;
  draft: boolean;
  html_url: string;
}

/**
 * The newest published (non-draft) release regardless of its `prerelease`
 * flag — used by the "prerelease" update channel to opt into whatever tag
 * shipped most recently, stable or not. Every tagged release (see
 * scripts/stage-release.sh) uploads its own `latest.json` asset alongside
 * the platform bundles, so once the tag is known here the same manifest
 * shape used by the stable channel can be fetched from that release's own
 * download URL instead of hand-building one from raw asset names.
 */
export async function fetchNewestReleaseTag(): Promise<GithubReleaseSummary> {
  const response = await fetch(GITHUB_RELEASES_API_URL, {
    headers: { Accept: "application/vnd.github+json" },
  });
  if (!response.ok) {
    throw new Error(`Releases lookup failed (${response.status})`);
  }
  const releases = (await response.json()) as GithubReleaseSummary[];
  const newest = releases.find((release) => !release.draft);
  if (!newest) {
    throw new Error("No published releases found on GitHub");
  }
  return newest;
}

export function manifestUrlForTag(tag: string): string {
  return `https://github.com/LunarVagabond/Pipe-Deck/releases/download/${tag}/latest.json`;
}

export function releasePageForTag(tag: string): string {
  return `https://github.com/LunarVagabond/Pipe-Deck/releases/tag/${tag}`;
}

export async function checkForUpdates(
  appInfo: AppInfo,
  channel: UpdateChannel = "latest",
): Promise<UpdateCheckResult> {
  const currentVersion = appInfo.releaseVersion;

  try {
    let manifestUrl = UPDATE_MANIFEST_URL;
    let releaseUrl = RELEASES_PAGE;
    if (channel === "prerelease") {
      const release = await fetchNewestReleaseTag();
      manifestUrl = manifestUrlForTag(release.tag_name);
      releaseUrl = release.html_url || releasePageForTag(release.tag_name);
    }

    const manifest = await fetchUpdateManifest(manifestUrl);
    const latestVersion = manifest.version?.replace(/^v/i, "") ?? "";
    if (!latestVersion) {
      return {
        status: "error",
        currentVersion: currentVersion ?? appInfo.buildRevision,
        error: "Update manifest has no version",
        canAutoInstall: false,
      };
    }

    if (!currentVersion) {
      // Dev builds carry a commit hash, not a semver tag, so there's nothing to
      // compare against — report the latest release instead of failing the check.
      return {
        status: "dev_build",
        currentVersion: appInfo.buildRevision,
        latestVersion,
        releaseUrl,
        canAutoInstall: false,
      };
    }

    const platformKey = platformKeyForInstallKind(appInfo.installKind);
    const platform = platformKey ? manifest.platforms[platformKey] : undefined;
    const downloadUrl = platform?.url;
    // The Tauri updater plugin's one-click AppImage auto-install path (installUpdate()
    // below, via @tauri-apps/plugin-updater's checkUpdater()) always checks the static
    // stable-channel endpoint baked into tauri.conf.json at build time — it has no way
    // to be pointed at a prerelease tag at runtime. Force the manual "Get update"
    // download-link flow instead of the auto-installer whenever this channel-aware
    // check resolved a prerelease release, so the two never disagree about which
    // build is about to be installed.
    const canAutoInstall =
      channel === "latest" &&
      appInfo.installKind === "app_image" &&
      Boolean(platform?.signature && downloadUrl);

    return {
      status: compareUpdateStatus(currentVersion, latestVersion),
      currentVersion,
      latestVersion,
      releaseUrl,
      downloadUrl,
      canAutoInstall,
      error:
        platformKey && !downloadUrl
          ? "No packaged download for this install type yet — use the releases page instead."
          : undefined,
    };
  } catch (error) {
    return {
      status: "error",
      currentVersion: currentVersion ?? appInfo.buildRevision,
      error: error instanceof Error ? error.message : String(error),
      canAutoInstall: false,
    };
  }
}

export type InstallProgress = number | null;

export async function installUpdate(
  result: UpdateCheckResult,
  onProgress?: (progress: InstallProgress) => void,
): Promise<void> {
  if (result.canAutoInstall) {
    const update = await checkUpdater();
    if (!update) {
      throw new Error("No update available from the updater plugin");
    }
    let contentLength: number | undefined;
    let downloaded = 0;
    await update.downloadAndInstall((event) => {
      switch (event.event) {
        case "Started":
          contentLength = event.data.contentLength ?? undefined;
          downloaded = 0;
          onProgress?.(contentLength ? 0 : null);
          break;
        case "Progress":
          downloaded += event.data.chunkLength;
          onProgress?.(
            contentLength
              ? Math.min(99, Math.round((downloaded / contentLength) * 100))
              : null,
          );
          break;
        case "Finished":
          onProgress?.(100);
          break;
      }
    });
    return;
  }

  const url = result.downloadUrl ?? result.releaseUrl;
  if (!url) {
    throw new Error(
      result.error ?? "No download URL available for this install type",
    );
  }

  await invoke("open_url", { url });
}

export const updateStatusLabel: Record<UpdateStatus, string> = {
  current: "Up to date",
  outdated: "Update available",
  severely_outdated: "Update strongly recommended",
  unknown: "Not checked yet",
  checking: "Checking…",
  error: "Update check failed",
  unsupported: "Managed externally",
  dev_build: "Dev build",
};
