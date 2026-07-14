import { invoke } from "@tauri-apps/api/core";
import { ref } from "vue";
import { useApplyResult } from "../stores/notices";

/** Mirrors `useMixerControls`'s debounce/pending-value pattern, keyed by link
 * id rather than device/stream id — one independent gain per connection
 * (issue #105), not per node. */
export function useConnectionEffects() {
  const { handleApplyResult } = useApplyResult();
  const pendingVolumes = ref<Record<string, number>>({});
  const debounceTimers: Record<string, number> = {};

  function clampVolume(value: number): number {
    return Math.min(100, Math.max(0, Math.round(value)));
  }

  async function addConnectionEffect(sourceId: string, targetDeviceId: string) {
    try {
      await invoke("add_connection_effect", { sourceId, targetDeviceId });
      handleApplyResult({ success: true }, "Volume control added");
    } catch (error) {
      handleApplyResult(
        { success: false, message: error instanceof Error ? error.message : String(error) },
        "",
      );
    }
  }

  async function removeConnectionEffect(sourceId: string, targetDeviceId: string) {
    try {
      await invoke("remove_connection_effect", { sourceId, targetDeviceId });
      handleApplyResult({ success: true }, "Volume control removed");
    } catch (error) {
      handleApplyResult(
        { success: false, message: error instanceof Error ? error.message : String(error) },
        "",
      );
    }
  }

  async function setConnectionVolume(sourceId: string, targetDeviceId: string, percent: number) {
    await invoke("set_connection_volume", { sourceId, targetDeviceId, percent });
  }

  /** Debounced connection-gain apply, mirrors `scheduleMixSourceVolume` but for
   * any connection's Volume effect rather than a virtual mic's mix sources. */
  function scheduleConnectionVolume(linkId: string, sourceId: string, targetDeviceId: string, percent: number) {
    const next = clampVolume(percent);
    pendingVolumes.value[linkId] = next;
    window.clearTimeout(debounceTimers[linkId]);
    debounceTimers[linkId] = window.setTimeout(() => {
      setConnectionVolume(sourceId, targetDeviceId, pendingVolumes.value[linkId]).catch((error) => {
        handleApplyResult(
          { success: false, message: error instanceof Error ? error.message : String(error) },
          "",
        );
      });
    }, 120);
  }

  async function toggleConnectionMute(sourceId: string, targetDeviceId: string, muted: boolean) {
    try {
      await invoke("set_connection_mute", { sourceId, targetDeviceId, muted: !muted });
      handleApplyResult({ success: true }, muted ? "Unmuted" : "Muted");
    } catch (error) {
      handleApplyResult(
        { success: false, message: error instanceof Error ? error.message : String(error) },
        "",
      );
    }
  }

  return {
    pendingVolumes,
    clampVolume,
    addConnectionEffect,
    removeConnectionEffect,
    setConnectionVolume,
    scheduleConnectionVolume,
    toggleConnectionMute,
  };
}
