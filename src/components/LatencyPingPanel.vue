<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { deviceNodeId, parseGraphNodeId, processingNodeNodeId, streamNodeId } from "./routing-graph/nodeIds";
import { findNodePath } from "./routing-graph/latencyPath";
import { streamDisplayLabel } from "../utils/routingLayout";
import type { LatencyPathNode, LatencyPingResult, RuntimeGraph } from "../types/graph";
import type { BuiltRoutingGraph } from "./routing-graph/buildGraph";

const props = defineProps<{
  graph: RuntimeGraph;
  edges: BuiltRoutingGraph["edges"];
}>();

const collapsed = ref(true);
const sourceGraphId = ref("");
const targetGraphId = ref("");
const loading = ref(false);
const error = ref<string | null>(null);
const result = ref<LatencyPingResult | null>(null);
const pathNotFound = ref(false);

interface NodeOption {
  graphId: string;
  label: string;
}

const nodeOptions = computed<NodeOption[]>(() => {
  const options: NodeOption[] = [];
  for (const device of props.graph.devices) {
    options.push({ graphId: deviceNodeId(device.id), label: device.label });
  }
  for (const stream of props.graph.streams) {
    options.push({ graphId: streamNodeId(stream.id), label: streamDisplayLabel(stream) });
  }
  for (const node of props.graph.processing_nodes ?? []) {
    options.push({ graphId: processingNodeNodeId(node.id), label: node.label });
  }
  return options.sort((a, b) => a.label.localeCompare(b.label));
});

const labelByGraphId = computed(() => {
  const map = new Map<string, string>();
  for (const option of nodeOptions.value) {
    map.set(option.graphId, option.label);
  }
  return map;
});

function labelForHopId(id: string): string {
  return labelByGraphId.value.get(deviceNodeId(id))
    ?? labelByGraphId.value.get(streamNodeId(id))
    ?? labelByGraphId.value.get(processingNodeNodeId(id))
    ?? id;
}

// Reset stale results whenever the picked endpoints change, so a leftover
// breakdown from a previous pair never looks like it belongs to the new one.
watch([sourceGraphId, targetGraphId], () => {
  result.value = null;
  pathNotFound.value = false;
  error.value = null;
});

function resolvePathNode(graphId: string): LatencyPathNode | null {
  const parsed = parseGraphNodeId(graphId);
  if (!parsed) return null;

  if (parsed.kind === "device") {
    const device = props.graph.devices.find((entry) => entry.id === parsed.id);
    if (!device) return null;
    return { id: device.id, system_name: device.system_name };
  }
  if (parsed.kind === "stream") {
    const stream = props.graph.streams.find((entry) => entry.id === parsed.id);
    if (!stream) return null;
    return { id: stream.id, system_name: stream.system_name };
  }
  const node = (props.graph.processing_nodes ?? []).find((entry) => entry.id === parsed.id);
  if (!node) return null;
  return { id: node.id, system_name: node.system_name };
}

async function measure() {
  if (!sourceGraphId.value || !targetGraphId.value) return;

  result.value = null;
  pathNotFound.value = false;
  error.value = null;

  const nodePath = findNodePath(sourceGraphId.value, targetGraphId.value, props.edges);
  if (!nodePath) {
    pathNotFound.value = true;
    return;
  }

  const path = nodePath.map(resolvePathNode).filter((entry): entry is LatencyPathNode => entry !== null);
  if (path.length !== nodePath.length) {
    error.value = "Could not resolve every node along the path.";
    return;
  }

  loading.value = true;
  try {
    result.value = await invoke<LatencyPingResult>("measure_latency_ping", { path });
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
}
</script>

<template>
  <div class="latency-ping-panel" :class="{ 'latency-ping-panel--collapsed': collapsed }">
    <button
      type="button"
      class="latency-ping-panel-toggle"
      :aria-expanded="!collapsed"
      @click="collapsed = !collapsed"
    >
      <span>Latency Ping</span>
      <span class="latency-ping-panel-toggle-icon">{{ collapsed ? "▲" : "▼" }}</span>
    </button>

    <div v-if="!collapsed" class="latency-ping-panel-body">
      <p class="latency-ping-panel-hint">
        Estimated buffering latency along a signal path, from PipeWire's own scheduling data — not a real
        measured round-trip.
      </p>

      <label class="latency-ping-panel-field">
        <span>Source</span>
        <select v-model="sourceGraphId" class="routing-select">
          <option value="" disabled>Choose a node&hellip;</option>
          <option v-for="option in nodeOptions" :key="option.graphId" :value="option.graphId">
            {{ option.label }}
          </option>
        </select>
      </label>

      <label class="latency-ping-panel-field">
        <span>Target</span>
        <select v-model="targetGraphId" class="routing-select">
          <option value="" disabled>Choose a node&hellip;</option>
          <option v-for="option in nodeOptions" :key="option.graphId" :value="option.graphId">
            {{ option.label }}
          </option>
        </select>
      </label>

      <button
        type="button"
        class="latency-ping-panel-measure"
        :disabled="!sourceGraphId || !targetGraphId || loading"
        @click="measure"
      >
        {{ loading ? "Measuring…" : "Measure" }}
      </button>

      <p v-if="pathNotFound" class="latency-ping-panel-message">No signal path between these two nodes.</p>
      <p v-else-if="error" class="latency-ping-panel-message latency-ping-panel-message--error">{{ error }}</p>

      <div v-else-if="result" class="latency-ping-panel-result">
        <p class="latency-ping-panel-total">
          <template v-if="result.total_latency_ms !== undefined">
            Total: {{ result.total_latency_ms.toFixed(2) }}ms
          </template>
          <template v-else>Total: incomplete — see hop with no data below</template>
        </p>
        <ul class="latency-ping-panel-hops">
          <li v-for="hop in result.hops" :key="hop.id">
            <span class="latency-ping-panel-hop-label">{{ labelForHopId(hop.id) }}</span>
            <span v-if="hop.latency_ms !== undefined" class="latency-ping-panel-hop-value">
              {{ hop.latency_ms.toFixed(2) }}ms ({{ hop.quantum }}/{{ hop.rate }})
            </span>
            <span v-else class="latency-ping-panel-hop-value latency-ping-panel-hop-value--missing">no data</span>
          </li>
        </ul>
      </div>
    </div>
  </div>
</template>
