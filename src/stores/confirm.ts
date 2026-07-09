import { ref } from "vue";

export interface ConfirmOptions {
  confirmLabel?: string;
  cancelLabel?: string;
  title?: string;
}

interface ConfirmState {
  open: boolean;
  message: string;
  title: string;
  confirmLabel: string;
  cancelLabel: string;
}

const confirmState = ref<ConfirmState>({
  open: false,
  message: "",
  title: "Confirm",
  confirmLabel: "Confirm",
  cancelLabel: "Cancel",
});

let resolver: ((value: boolean) => void) | null = null;

export function useConfirm() {
  function confirm(message: string, options: ConfirmOptions = {}): Promise<boolean> {
    return new Promise((resolve) => {
      resolver = resolve;
      confirmState.value = {
        open: true,
        message,
        title: options.title ?? "Confirm",
        confirmLabel: options.confirmLabel ?? "Confirm",
        cancelLabel: options.cancelLabel ?? "Cancel",
      };
    });
  }

  function answer(confirmed: boolean) {
    confirmState.value = { ...confirmState.value, open: false };
    resolver?.(confirmed);
    resolver = null;
  }

  return { confirmState, confirm, answer };
}
