import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { DaemonStatus } from "../types/graph";

// Module-level singleton so the sidebar status tray and Settings > Background
// both read the same status, and a refresh triggered from either place is
// immediately reflected in the other — previously each held its own copy, so
// toggling restore-at-login in Settings never updated the sidebar dot until
// the next app launch.
const daemonStatus = ref<DaemonStatus | null>(null);

export function useDaemonStatus() {
  async function refreshDaemonStatus() {
    try {
      daemonStatus.value = await invoke<DaemonStatus>("get_daemon_status");
    } catch {
      daemonStatus.value = null;
    }
  }

  const restoreAtLoginText = computed(() => {
    const status = daemonStatus.value;
    if (!status?.enabled) return "Disabled";
    if (status.running && !status.last_error) return "Active";
    return "Unhealthy";
  });

  const restoreAtLoginClass = computed(() => {
    const status = daemonStatus.value;
    if (!status?.enabled) return "status-dot--muted";
    if (status.running && !status.last_error) return "status-dot--ok";
    return "status-dot--error";
  });

  return { daemonStatus, refreshDaemonStatus, restoreAtLoginText, restoreAtLoginClass };
}
