import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useApplyResult } from "../stores/notices";

export type ApplyResultPayload = { success: boolean; message?: string };

export function useRoutingActions() {
  const { handleApplyResult } = useApplyResult();
  const canUndo = ref(false);

  async function refreshCanUndo() {
    try {
      canUndo.value = await invoke<boolean>("can_undo_routing");
    } catch {
      canUndo.value = false;
    }
  }

  async function setStreamTarget(
    streamId: string,
    targetDeviceId: string,
    successMessage = "Routing updated",
  ): Promise<ApplyResultPayload | null> {
    try {
      const result = await invoke<ApplyResultPayload>("set_stream_target", {
        streamId,
        targetDeviceId,
      });
      handleApplyResult(result, successMessage);
      return result;
    } catch (error) {
      const payload = {
        success: false,
        message: error instanceof Error ? error.message : String(error),
      };
      handleApplyResult(payload, "");
      return payload;
    }
  }

  async function setDeviceRoute(
    sourceDeviceId: string,
    targetDeviceId: string,
    successMessage = "Device routing updated",
  ): Promise<ApplyResultPayload | null> {
    try {
      const result = await invoke<ApplyResultPayload>("set_device_route", {
        sourceDeviceId,
        targetDeviceId,
      });
      handleApplyResult(result, successMessage);
      return result;
    } catch (error) {
      const payload = {
        success: false,
        message: error instanceof Error ? error.message : String(error),
      };
      handleApplyResult(payload, "");
      return payload;
    }
  }

  async function setDeviceTargets(
    sourceDeviceId: string,
    targetDeviceIds: string[],
    successMessage = "Sink routing updated",
  ): Promise<ApplyResultPayload | null> {
    try {
      const result = await invoke<ApplyResultPayload>("set_device_targets", {
        sourceDeviceId,
        targetDeviceIds,
      });
      handleApplyResult(result, successMessage);
      return result;
    } catch (error) {
      const payload = {
        success: false,
        message: error instanceof Error ? error.message : String(error),
      };
      handleApplyResult(payload, "");
      return payload;
    }
  }

  async function clearStreamTarget(
    streamId: string,
    previousTargetDeviceId: string,
    successMessage = "Routing cleared",
  ): Promise<ApplyResultPayload | null> {
    try {
      const result = await invoke<ApplyResultPayload>("clear_stream_target", {
        streamId,
        previousTargetDeviceId,
      });
      handleApplyResult(result, successMessage);
      return result;
    } catch (error) {
      const payload = {
        success: false,
        message: error instanceof Error ? error.message : String(error),
      };
      handleApplyResult(payload, "");
      return payload;
    }
  }

  async function undoLastRouting() {
    if (!canUndo.value) {
      return;
    }
    try {
      await invoke("undo_last_routing");
      await refreshCanUndo();
    } catch {
      // callers may surface notices from matrix/graph views
    }
  }

  return {
    canUndo,
    refreshCanUndo,
    setStreamTarget,
    setDeviceRoute,
    setDeviceTargets,
    clearStreamTarget,
    undoLastRouting,
  };
}
