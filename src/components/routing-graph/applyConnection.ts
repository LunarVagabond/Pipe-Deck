import type { Connection } from "@vue-flow/core";
import { invoke } from "@tauri-apps/api/core";
import type { RuntimeGraph } from "../../types/graph";
import {
  type ConnectionContext,
  type PreviousEdge,
  type RoutingConnectionAction,
  resolveConnectionAction,
} from "./connectionRules";

export type ApplyRoutingResultHandler = (
  result: { success: boolean; message?: string },
  successMessage: string,
) => void;

/** Applies a single, already-resolved action (never `processing_node_retarget`
 * itself — that's a wrapper `applyResolvedAction` unwraps before getting
 * here) and reports the result. Shared by the top-level dispatch and by
 * `processing_node_retarget`'s "then" step so both go through identical
 * invoke/report logic. */
async function invokeSingleAction(
  action: Exclude<RoutingConnectionAction, { type: "processing_node_retarget" }>,
  onResult: ApplyRoutingResultHandler,
): Promise<void> {
  if (action.type === "stream_target") {
    const response = await invoke<{ success: boolean; message?: string }>("set_stream_target", {
      streamId: action.streamId,
      targetDeviceId: action.targetDeviceId,
    });
    onResult(response, "Routing updated");
  } else if (action.type === "clear_stream_target") {
    const response = await invoke<{ success: boolean; message?: string }>("clear_stream_target", {
      streamId: action.streamId,
      previousTargetDeviceId: action.previousTargetDeviceId,
    });
    onResult(response, "Routing cleared");
  } else if (action.type === "device_targets") {
    const response = await invoke<{ success: boolean; message?: string }>("set_device_targets", {
      sourceDeviceId: action.sourceDeviceId,
      targetDeviceIds: action.targetDeviceIds,
    });
    onResult(response, "Sink routing updated");
  } else if (action.type === "stream_mic_passthrough_add") {
    const response = await invoke<{ success: boolean; message?: string }>("enable_stream_mic_passthrough", {
      streamId: action.streamId,
      micDeviceId: action.micDeviceId,
    });
    onResult(response, "Also sending this app's audio to your microphone");
  } else if (action.type === "processing_node_connect") {
    const response = await invoke<{ success: boolean; message?: string }>("connect_processing_node_port", {
      nodeId: action.nodeId,
      direction: action.direction,
      peerId: action.peerId,
    });
    onResult(response, "Routing updated");
  } else if (action.type === "processing_node_disconnect") {
    const response = await invoke<{ success: boolean; message?: string }>("disconnect_processing_node_port", {
      nodeId: action.nodeId,
      direction: action.direction,
      portIndex: action.portIndex,
    });
    onResult(response, "Routing cleared");
  } else {
    const response = await invoke<{ success: boolean; message?: string }>("set_device_route", {
      sourceDeviceId: action.sourceDeviceId,
      targetDeviceId: action.targetDeviceId,
    });
    onResult(response, "Device routing updated");
  }
}

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
    if (result.action.type === "processing_node_retarget") {
      const disconnectResponse = await invoke<{ success: boolean; message?: string }>(
        "disconnect_processing_node_port",
        {
          nodeId: result.action.disconnect.nodeId,
          direction: result.action.disconnect.direction,
          portIndex: result.action.disconnect.portIndex,
        },
      );
      if (!disconnectResponse.success) {
        onResult(disconnectResponse, "");
        return false;
      }
      await invokeSingleAction(result.action.then, onResult);
    } else {
      await invokeSingleAction(result.action, onResult);
    }
    return true;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    onResult({ success: false, message: `Couldn't update routing: ${message}` }, "");
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
