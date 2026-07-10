import type { Connection } from "@vue-flow/core";
import { invoke } from "@tauri-apps/api/core";
import type { RuntimeGraph } from "../../types/graph";
import {
  type ConnectionContext,
  type PreviousEdge,
  resolveConnectionAction,
} from "./connectionRules";

export type ApplyRoutingResultHandler = (
  result: { success: boolean; message?: string },
  successMessage: string,
) => void;

export async function applyRoutingConnection(
  graph: RuntimeGraph,
  connection: Connection,
  onResult: ApplyRoutingResultHandler,
  context: ConnectionContext = { mode: "connect" },
): Promise<boolean> {
  const result = resolveConnectionAction(graph, connection, context);
  if ("error" in result) {
    onResult({ success: false, message: result.error }, "");
    return false;
  }

  try {
    if (result.action.type === "stream_target") {
      const response = await invoke<{ success: boolean; message?: string }>("set_stream_target", {
        streamId: result.action.streamId,
        targetDeviceId: result.action.targetDeviceId,
      });
      onResult(response, "Routing updated");
    } else if (result.action.type === "clear_stream_target") {
      const response = await invoke<{ success: boolean; message?: string }>("clear_stream_target", {
        streamId: result.action.streamId,
        previousTargetDeviceId: result.action.previousTargetDeviceId,
      });
      onResult(response, "Routing cleared");
    } else if (result.action.type === "device_targets") {
      const response = await invoke<{ success: boolean; message?: string }>("set_device_targets", {
        sourceDeviceId: result.action.sourceDeviceId,
        targetDeviceIds: result.action.targetDeviceIds,
      });
      onResult(response, "Sink routing updated");
    } else {
      const response = await invoke<{ success: boolean; message?: string }>("set_device_route", {
        sourceDeviceId: result.action.sourceDeviceId,
        targetDeviceId: result.action.targetDeviceId,
      });
      onResult(response, "Device routing updated");
    }
    return true;
  } catch (error) {
    onResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
    return false;
  }
}

export async function applyEdgeDisconnect(
  graph: RuntimeGraph,
  previousEdge: PreviousEdge,
  onResult: ApplyRoutingResultHandler,
): Promise<boolean> {
  return applyRoutingConnection(
    graph,
    {
      source: previousEdge.source,
      target: previousEdge.target,
      sourceHandle: previousEdge.sourceHandle ?? null,
      targetHandle: previousEdge.targetHandle ?? null,
    },
    onResult,
    { mode: "edge_disconnect", previousEdge },
  );
}
