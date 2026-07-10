import { ref } from "vue";

export interface PromptOptions {
  title?: string;
  message?: string;
  defaultValue?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  placeholder?: string;
}

interface PromptState {
  open: boolean;
  title: string;
  message: string;
  value: string;
  confirmLabel: string;
  cancelLabel: string;
  placeholder: string;
}

const promptState = ref<PromptState>({
  open: false,
  title: "",
  message: "",
  value: "",
  confirmLabel: "Save",
  cancelLabel: "Cancel",
  placeholder: "",
});

let resolver: ((value: string | null) => void) | null = null;

export function usePrompt() {
  function prompt(options: PromptOptions = {}): Promise<string | null> {
    return new Promise((resolve) => {
      resolver = resolve;
      promptState.value = {
        open: true,
        title: options.title ?? "Input",
        message: options.message ?? "",
        value: options.defaultValue ?? "",
        confirmLabel: options.confirmLabel ?? "Save",
        cancelLabel: options.cancelLabel ?? "Cancel",
        placeholder: options.placeholder ?? "",
      };
    });
  }

  function answer(value: string | null) {
    promptState.value = { ...promptState.value, open: false };
    resolver?.(value);
    resolver = null;
  }

  function updateValue(value: string) {
    promptState.value = { ...promptState.value, value };
  }

  return { promptState, prompt, answer, updateValue };
}
