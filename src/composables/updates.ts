import type { UpdateCheckResult, UpdateStatus } from "../types/app";

const RELEASES_API =
  "https://api.github.com/repos/LunarVagabond/Pipe-Deck/releases/latest";
const RELEASES_PAGE = "https://github.com/LunarVagabond/Pipe-Deck/releases/latest";

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

export async function checkForUpdates(currentVersion: string): Promise<UpdateCheckResult> {
  try {
    const response = await fetch(RELEASES_API, {
      headers: { Accept: "application/vnd.github+json" },
    });

    if (!response.ok) {
      return {
        status: "unknown",
        currentVersion,
        error: `Update check failed (${response.status})`,
      };
    }

    const payload = (await response.json()) as { tag_name?: string; html_url?: string };
    const latestVersion = payload.tag_name?.replace(/^v/i, "") ?? "";
    const releaseUrl = payload.html_url ?? RELEASES_PAGE;

    if (!latestVersion) {
      return {
        status: "unknown",
        currentVersion,
        error: "No published release found yet",
      };
    }

    return {
      status: compareUpdateStatus(currentVersion, latestVersion),
      currentVersion,
      latestVersion,
      releaseUrl,
    };
  } catch (error) {
    return {
      status: "unknown",
      currentVersion,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

export const updateStatusLabel: Record<UpdateStatus, string> = {
  current: "Up to date",
  outdated: "Update available",
  severely_outdated: "Update strongly recommended",
  unknown: "Could not check",
  checking: "Checking…",
};
