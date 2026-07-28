import { ref } from "vue";

// Module-level singleton (same pattern as stores/newDeviceDialog.ts) so the
// modal can be triggered from anywhere — the global `?` keydown listener in
// App.vue, or the topbar's help button — without threading a v-model through
// unrelated components.
const open = ref(false);

export function useShortcutsModal() {
  function openShortcutsModal() {
    open.value = true;
  }

  function closeShortcutsModal() {
    open.value = false;
  }

  function toggleShortcutsModal() {
    open.value = !open.value;
  }

  return { shortcutsModalOpen: open, openShortcutsModal, closeShortcutsModal, toggleShortcutsModal };
}
