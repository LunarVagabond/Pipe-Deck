import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { RuntimeGraph } from "../types/graph";
import type { BuiltRoutingGraph } from "../components/routing-graph/buildGraph";
import { parseGraphNodeId } from "../components/routing-graph/nodeIds";
import { computeConnectedComponent } from "../components/routing-graph/connectedComponent";
import { useApplyResult } from "../stores/notices";

/**
 * Processing node kinds "Isolate" is allowed to bypass. Mixer/Fan-out are
 * structural, not effects — isolating an EQ must never touch them (#222).
 * `delay` (issue #313) is a real DSP-backed effect kind, same as `eq5band`.
 * `stub` is included so a future real effect kind picks up isolate support
 * for free; it's a no-op today since stub nodes don't process audio.
 */
const ISOLATABLE_KINDS = new Set(["eq5band", "delay", "stub"]);

async function setBypassed(nodeId: string, bypassed: boolean, handleApplyResult: ReturnType<typeof useApplyResult>["handleApplyResult"]) {
  const response = await invoke<{ success: boolean; message?: string }>("set_processing_node_bypassed", {
    nodeId,
    bypassed,
  });
  if (!response.success) {
    handleApplyResult(response, "");
  }
}

export function useEffectIsolation() {
  const { handleApplyResult } = useApplyResult();
  const isolatedNodeId = ref<string | null>(null);
  let bypassedByIsolate: string[] = [];

  async function clearIsolation() {
    for (const id of bypassedByIsolate) {
      try {
        await setBypassed(id, false, handleApplyResult);
      } catch {
        // Node was likely deleted while isolated — nothing left to restore.
      }
    }
    bypassedByIsolate = [];
    isolatedNodeId.value = null;
  }

  async function activateIsolation(nodeId: string, graph: RuntimeGraph, edges: BuiltRoutingGraph["edges"]) {
    const connected = computeConnectedComponent(nodeId, edges);
    const targets: string[] = [];
    for (const graphId of connected) {
      if (graphId === nodeId) continue;
      const parsed = parseGraphNodeId(graphId);
      if (!parsed || parsed.kind !== "processingNode") continue;
      const node = (graph.processing_nodes ?? []).find((entry) => entry.id === parsed.id);
      if (!node || !ISOLATABLE_KINDS.has(node.kind.kind) || node.bypassed) continue;
      targets.push(node.id);
    }
    for (const id of targets) {
      await setBypassed(id, true, handleApplyResult);
    }
    bypassedByIsolate = targets;
    isolatedNodeId.value = nodeId;
  }

  async function toggleIsolation(nodeId: string, graph: RuntimeGraph, edges: BuiltRoutingGraph["edges"]) {
    const wasIsolatingThisNode = isolatedNodeId.value === nodeId;
    if (isolatedNodeId.value) {
      await clearIsolation();
    }
    if (!wasIsolatingThisNode) {
      await activateIsolation(nodeId, graph, edges);
    }
  }

  return { isolatedNodeId, toggleIsolation, clearIsolation };
}
