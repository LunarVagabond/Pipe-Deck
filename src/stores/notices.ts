import { ref } from "vue";

export type NoticeKind = "info" | "success" | "warn" | "error";

export interface AppNotice {
  id: number;
  kind: NoticeKind;
  message: string;
}

const notices = ref<AppNotice[]>([]);
let nextId = 1;

export function useNotices() {
  function pushNotice(kind: NoticeKind, message: string, timeoutMs = 5000) {
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
