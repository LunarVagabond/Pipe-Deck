import { ref } from "vue";

interface NewDeviceDialogState {
  open: boolean;
  type: "input" | "output";
}

// Module-level singleton (same pattern as stores/prompt.ts) so the dialog can
// be triggered from anywhere — the app-level "+ New" toolbar button, or the
// routing graph's right-click "add node here" menu — with an optional preset
// type, without threading a v-model through unrelated components.
const state = ref<NewDeviceDialogState>({ open: false, type: "output" });

export function useNewDeviceDialog() {
  function openNewDeviceDialog(type: "input" | "output" = "output") {
    state.value = { open: true, type };
  }

  function closeNewDeviceDialog() {
    state.value = { ...state.value, open: false };
  }

  return { newDeviceDialogState: state, openNewDeviceDialog, closeNewDeviceDialog };
}
