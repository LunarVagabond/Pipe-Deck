import { ref } from "vue";

interface NewDeviceDialogState {
  open: boolean;
  type: "input" | "output";
  multi: boolean;
}

// Module-level singleton (same pattern as stores/prompt.ts) so the dialog can
// be triggered from anywhere — the app-level "+ New" toolbar button, or the
// routing graph's right-click "add node here" menu — with an optional preset
// type/mode, without threading a v-model through unrelated components.
const state = ref<NewDeviceDialogState>({ open: false, type: "output", multi: false });

export function useNewDeviceDialog() {
  function openNewDeviceDialog(type: "input" | "output" = "output", multi = false) {
    state.value = { open: true, type, multi };
  }

  function closeNewDeviceDialog() {
    state.value = { ...state.value, open: false };
  }

  return { newDeviceDialogState: state, openNewDeviceDialog, closeNewDeviceDialog };
}
