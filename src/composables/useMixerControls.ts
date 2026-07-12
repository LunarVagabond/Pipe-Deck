import { invoke } from "@tauri-apps/api/core";
import { ref } from "vue";
import { useApplyResult } from "../stores/notices";

export type ApplyResultPayload = { success: boolean; message?: string };

export function useMixerControls() {
  const { handleApplyResult } = useApplyResult();
  const pendingVolumes = ref<Record<string, number>>({});
  const debounceTimers: Record<string, number> = {};

  function clampVolume(value: number): number {
    return Math.min(100, Math.max(0, Math.round(value)));
  }

  /** Debounced volume apply: schedules the network call and tracks an optimistic display value. */
  function scheduleChannelVolume(
    channelType: "device" | "stream",
    id: string,
    percent: number,
  ) {
    const next = clampVolume(percent);
    pendingVolumes.value[id] = next;
    window.clearTimeout(debounceTimers[id]);
    debounceTimers[id] = window.setTimeout(() => {
      void applyChannelVolume(channelType, id, pendingVolumes.value[id]);
    }, 120);
  }

  async function setDeviceVolume(deviceId: string, percent: number) {
    await invoke("set_device_volume", { deviceId, percent });
  }

  async function setStreamVolume(streamId: string, percent: number) {
    await invoke("set_stream_volume", { streamId, percent });
  }

  async function setDeviceMute(deviceId: string, muted: boolean) {
    await invoke("set_device_mute", { deviceId, muted });
  }

  async function setStreamMute(streamId: string, muted: boolean) {
    await invoke("set_stream_mute", { streamId, muted });
  }

  async function applyChannelVolume(
    channelType: "device" | "stream",
    id: string,
    percent: number,
    onError?: (message: string) => void,
  ) {
    try {
      if (channelType === "stream") {
        await setStreamVolume(id, percent);
      } else {
        await setDeviceVolume(id, percent);
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (onError) {
        onError(message);
      } else {
        handleApplyResult({ success: false, message }, "");
      }
    }
  }

  async function toggleChannelMute(
    channelType: "device" | "stream",
    id: string,
    muted: boolean,
    successMessage?: string,
  ) {
    try {
      if (channelType === "stream") {
        await setStreamMute(id, !muted);
      } else {
        await setDeviceMute(id, !muted);
      }
      handleApplyResult({ success: true }, successMessage ?? (muted ? "Unmuted" : "Muted"));
    } catch (error) {
      handleApplyResult(
        { success: false, message: error instanceof Error ? error.message : String(error) },
        "",
      );
    }
  }

  return {
    setDeviceVolume,
    setStreamVolume,
    setDeviceMute,
    setStreamMute,
    applyChannelVolume,
    toggleChannelMute,
    pendingVolumes,
    clampVolume,
    scheduleChannelVolume,
  };
}
