import type { RecentStreamIdentity } from "../types/graph";

export function filterRecentlySeen(
  entries: RecentStreamIdentity[] | undefined,
): RecentStreamIdentity[] {
  return (entries ?? []).filter((entry) => !entry.is_live && !entry.is_system);
}

export function recentEntryLabel(entry: RecentStreamIdentity): string {
  if (entry.media_name && entry.media_name !== entry.app_name) {
    return `${entry.app_name} (${entry.media_name})`;
  }
  return entry.app_name;
}

export function recentEntryAgo(entry: RecentStreamIdentity): string {
  const seconds = Math.max(0, Math.floor(Date.now() / 1000) - entry.last_seen_secs);
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ago`;
}
