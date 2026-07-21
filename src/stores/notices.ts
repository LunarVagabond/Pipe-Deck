import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { Preferences } from "../types/graph";

export type NoticeKind = "info" | "success" | "warn" | "error";

export interface AppNotice {
  id: number;
  kind: NoticeKind;
  message: string;
}

export const DEFAULT_NOTICE_DURATION_MS = 5000;

const notices = ref<AppNotice[]>([]);
// Module-level singleton so every pushNotice() call (across views/stores)
// shares one configured duration without threading it through every caller.
const noticeDurationMs = ref(DEFAULT_NOTICE_DURATION_MS);
let nextId = 1;

export function useNotices() {
  function pushNotice(kind: NoticeKind, message: string, timeoutMs = noticeDurationMs.value) {
    const notice: AppNotice = { id: nextId++, kind, message };
    notices.value = [notice, ...notices.value].slice(0, 4);
    if (timeoutMs > 0) {
      window.setTimeout(() => dismissNotice(notice.id), timeoutMs);
    }
  }

  function dismissNotice(id: number) {
    notices.value = notices.value.filter((notice) => notice.id !== id);
  }

  return { notices, pushNotice, dismissNotice };
}

export function useNoticeSettings() {
  const { handleApplyResult } = useApplyResult();

  async function initNoticeSettings() {
    try {
      const config = await invoke<{ preferences?: Preferences }>("get_config");
      noticeDurationMs.value = config.preferences?.notice_duration_ms ?? DEFAULT_NOTICE_DURATION_MS;
    } catch {
      // Static default stands as the fallback if config load fails.
    }
  }

  async function setNoticeDuration(ms: number) {
    const previous = noticeDurationMs.value;
    noticeDurationMs.value = ms;
    try {
      await invoke("set_notice_duration_ms", { ms });
    } catch (error) {
      noticeDurationMs.value = previous;
      handleApplyResult(
        { success: false, message: error instanceof Error ? error.message : String(error) },
        "",
      );
    }
  }

  return { noticeDurationMs, initNoticeSettings, setNoticeDuration };
}

export function useApplyResult() {
  const { pushNotice } = useNotices();

  function handleApplyResult(result: { success: boolean; message?: string }, successMessage: string) {
    if (result.success) {
      pushNotice("success", successMessage);
      return true;
    }
    pushNotice("error", result.message ?? "Operation failed");
    return false;
  }

  return { handleApplyResult };
}
