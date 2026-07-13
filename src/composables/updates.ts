import { invoke } from "@tauri-apps/api/core";
import { check as checkUpdater } from "@tauri-apps/plugin-updater";
import type { AppInfo, InstallKind, UpdateCheckResult, UpdateStatus } from "../types/app";

export const UPDATE_MANIFEST_URL =
  "https://github.com/LunarVagabond/Pipe-Deck/releases/latest/download/latest.json";
export const RELEASES_PAGE = "https://github.com/LunarVagabond/Pipe-Deck/releases/latest";

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

export function compareUpdateStatus(current: string, latest: string): UpdateStatus {
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

export function platformKeyForInstallKind(installKind: InstallKind): string | null {
  switch (installKind) {
    case "app_image":
      return "linux-x86_64-appimage";
    case "deb":
      return "linux-x86_64-deb";
    case "rpm":
      return "linux-x86_64-rpm";
    case "flatpak":
      return "linux-x86_64-flatpak";
    case "native":
    case "dev":
      return "linux-x86_64-binary";
    default:
      return null;
  }
}

export async function fetchUpdateManifest(): Promise<UpdateManifest> {
  const response = await fetch(UPDATE_MANIFEST_URL, {
    headers: { Accept: "application/json" },
  });
  if (!response.ok) {
    throw new Error(`Update manifest fetch failed (${response.status})`);
  }
  return (await response.json()) as UpdateManifest;
}

export async function checkForUpdates(appInfo: AppInfo): Promise<UpdateCheckResult> {
  const currentVersion = appInfo.releaseVersion;
  if (!currentVersion) {
    return {
      status: "error",
      currentVersion: appInfo.buildRevision,
      error: "Update check requires a tagged release build",
      canAutoInstall: false,
    };
  }

  if (appInfo.installKind === "flatpak") {
    return {
      status: "unsupported",
      currentVersion,
      error: "Flatpak builds update through Flathub — check there for the latest version.",
      canAutoInstall: false,
    };
  }

  try {
    const manifest = await fetchUpdateManifest();
    const latestVersion = manifest.version?.replace(/^v/i, "") ?? "";
    if (!latestVersion) {
      return {
        status: "error",
        currentVersion,
        error: "Update manifest has no version",
        canAutoInstall: false,
      };
    }

    const platformKey = platformKeyForInstallKind(appInfo.installKind);
    const platform = platformKey ? manifest.platforms[platformKey] : undefined;
    const downloadUrl = platform?.url;
    const canAutoInstall =
      appInfo.installKind === "app_image" && Boolean(platform?.signature && downloadUrl);

    return {
      status: compareUpdateStatus(currentVersion, latestVersion),
      currentVersion,
      latestVersion,
      releaseUrl: RELEASES_PAGE,
      downloadUrl,
      canAutoInstall,
      error: platformKey && !downloadUrl ? `No update package for ${platformKey}` : undefined,
    };
  } catch (error) {
    return {
      status: "error",
      currentVersion,
      error: error instanceof Error ? error.message : String(error),
      canAutoInstall: false,
    };
  }
}

export async function installUpdate(result: UpdateCheckResult): Promise<void> {
  if (result.canAutoInstall) {
    const update = await checkUpdater();
    if (!update) {
      throw new Error("No update available from the updater plugin");
    }
    await update.downloadAndInstall();
    return;
  }

  const url = result.downloadUrl ?? result.releaseUrl;
  if (!url) {
    throw new Error(result.error ?? "No download URL available for this install type");
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
};
