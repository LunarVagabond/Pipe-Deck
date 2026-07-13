import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { checkForUpdates, installUpdate, updateStatusLabel } from "../composables/updates";
import type { AppInfo, UpdateCheckResult, UpdateStatus } from "../types/app";

// Module-level singleton state so the sidebar status tray and the Settings > About
// tab both read/drive the exact same check, instead of each fetching independently.
const appInfo = ref<AppInfo | null>(null);
const updateResult = ref<UpdateCheckResult | null>(null);
const checkingUpdates = ref(false);

const updateStatus = computed<UpdateStatus>(() => {
  if (checkingUpdates.value) return "checking";
  return updateResult.value?.status ?? "unknown";
});

const updateStatusText = computed(() => updateStatusLabel[updateStatus.value]);

export function useUpdateStatus() {
  async function ensureAppInfo(): Promise<AppInfo | null> {
    if (appInfo.value) return appInfo.value;
    try {
      appInfo.value = await invoke<AppInfo>("get_app_info");
    } catch {
      appInfo.value = null;
    }
    return appInfo.value;
  }

  async function checkForUpdatesNow() {
    const info = await ensureAppInfo();
    if (!info) return;
    checkingUpdates.value = true;
    try {
      updateResult.value = await checkForUpdates(info);
    } finally {
      checkingUpdates.value = false;
    }
  }

  async function installUpdateNow() {
    if (!updateResult.value) return;
    await installUpdate(updateResult.value);
  }

  return {
    appInfo,
    updateResult,
    checkingUpdates,
    updateStatus,
    updateStatusText,
    ensureAppInfo,
    checkForUpdatesNow,
    installUpdateNow,
  };
}
