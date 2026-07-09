<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import NodeCardHeader from "./NodeCardHeader.vue";
import { useApplyResult } from "../stores/notices";
import type { Device, RuntimeGraph, Stream } from "../types/graph";
import {
  deviceColumn,
  deviceSubtitle,
  linkColor,
  streamAccent,
  streamSubtitle,
  targetLabel,
  type MatrixNode,
} from "../utils/routingLayout";

const { graph } = defineProps<{
  graph: RuntimeGraph;
}>();

const { handleApplyResult } = useApplyResult();
const matrixRef = ref<HTMLElement | null>(null);
const nodeRefs = ref<Record<string, HTMLElement | null>>({});

interface LinePath {
  id: string;
  d: string;
  color: string;
}

const lines = ref<LinePath[]>([]);

const columns = computed(() => {
  const apps: MatrixNode[] = graph.streams.map((stream) => ({
    id: stream.id,
    label: stream.app_name,
    column: "applications" as const,
    accent: streamAccent(stream.id),
    subtitle: streamSubtitle(stream),
  }));

  const routing: MatrixNode[] = [];
  const outputs: MatrixNode[] = [];
  const inputs: MatrixNode[] = [];

  for (const device of graph.devices) {
    const column = deviceColumn(device);
    if (!column) continue;

    const node: MatrixNode = {
      id: device.id,
      label: device.label,
      column,
      subtitle: deviceSubtitle(device),
      accent: accentForDevice(device),
    };

    if (column === "routing") routing.push(node);
    if (column === "outputs") outputs.push(node);
    if (column === "inputs") inputs.push(node);
  }

  return { apps, routing, outputs, inputs };
});

function targetsForStream(stream: Stream) {
  return graph.devices.filter((device) => {
    if (device.system_name.startsWith("pipe-deck-feed-")) return false;
    if (stream.direction === "playback") {
      return (
        device.direction === "output" ||
        device.direction === "duplex" ||
        (device.kind === "virtual" && device.direction === "input")
      );
    }
    return device.direction === "input" || device.direction === "duplex";
  });
}

function targetsForVirtualSink(device: Device) {
  return graph.devices.filter((candidate) => {
    if (candidate.id === device.id) return false;
    if (candidate.kind === "physical" && candidate.direction === "output") {
      return true;
    }
    return candidate.kind === "virtual" && candidate.direction === "input";
  });
}

function deviceById(id: string) {
  return graph.devices.find((device) => device.id === id);
}

function setNodeRef(id: string, el: HTMLElement | null) {
  if (el) nodeRefs.value[id] = el;
  else delete nodeRefs.value[id];
}

function nodeCenter(el: HTMLElement, side: "left" | "right") {
  const matrix = matrixRef.value;
  if (!matrix) return { x: 0, y: 0 };

  const matrixRect = matrix.getBoundingClientRect();
  const rect = el.getBoundingClientRect();
  const x =
    side === "right"
      ? rect.right - matrixRect.left
      : rect.left - matrixRect.left;
  const y = rect.top - matrixRect.top + rect.height / 2;
  return { x, y };
}

function buildPath(
  from: { x: number; y: number },
  to: { x: number; y: number },
): string {
  const dx = Math.max((to.x - from.x) * 0.45, 40);
  return `M ${from.x} ${from.y} C ${from.x + dx} ${from.y}, ${to.x - dx} ${to.y}, ${to.x} ${to.y}`;
}

async function updateLines() {
  await nextTick();

  const nextLines: LinePath[] = [];
  for (const link of graph.links) {
    const sourceEl = nodeRefs.value[link.source_id];
    const targetEl = nodeRefs.value[link.target_id];
    if (!sourceEl || !targetEl) continue;

    const from = nodeCenter(sourceEl, "right");
    const to = nodeCenter(targetEl, "left");

    nextLines.push({
      id: link.id,
      d: buildPath(from, to),
      color: linkColor(link.source_id, link.target_id),
    });
  }

  lines.value = nextLines;
}

async function onDeviceRouteChange(sourceDeviceId: string, event: Event) {
  const targetDeviceId = (event.target as HTMLSelectElement).value;
  if (!targetDeviceId) return;
  try {
    const result = await invoke<{ success: boolean; message?: string }>("set_device_route", {
      sourceDeviceId,
      targetDeviceId,
    });
    handleApplyResult(result, "Device routing updated");
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

async function onTargetChange(streamId: string, event: Event) {
  const targetDeviceId = (event.target as HTMLSelectElement).value;
  if (!targetDeviceId) return;
  try {
    const result = await invoke<{ success: boolean; message?: string }>("set_stream_target", {
      streamId,
      targetDeviceId,
    });
    handleApplyResult(result, "Routing updated");
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

async function saveRename(device: Device, alias: string) {
  try {
    await invoke("set_device_alias", { systemName: device.system_name, alias });
    handleApplyResult({ success: true }, "Device renamed");
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

async function removeVirtual(device: Device) {
  if (!window.confirm(`Delete virtual device "${device.label}"?`)) {
    return;
  }

  try {
    await invoke("remove_virtual_device", { systemName: device.system_name });
    handleApplyResult({ success: true }, "Virtual device removed");
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

let resizeObserver: ResizeObserver | null = null;

onMounted(() => {
  updateLines();
  if (matrixRef.value) {
    resizeObserver = new ResizeObserver(() => updateLines());
    resizeObserver.observe(matrixRef.value);
  }
  window.addEventListener("resize", updateLines);
});

onUnmounted(() => {
  resizeObserver?.disconnect();
  window.removeEventListener("resize", updateLines);
});

watch(() => graph, () => updateLines(), { deep: true });

function accentForDevice(device: Device): string | undefined {
  if (device.kind === "virtual" && device.direction === "output") {
    return streamAccent(device.id);
  }
  return undefined;
}
</script>

<template>
  <div ref="matrixRef" class="routing-matrix">
    <svg class="connections" aria-hidden="true">
      <path
        v-for="line in lines"
        :key="line.id"
        :d="line.d"
        :stroke="line.color"
        stroke-width="2"
        fill="none"
        stroke-opacity="0.85"
      />
    </svg>

    <div class="columns">
      <section class="column">
        <h3>Applications</h3>
        <div
          v-for="stream in graph.streams"
          :key="stream.id"
          :ref="(el) => setNodeRef(stream.id, el as HTMLElement | null)"
          class="node"
        >
          <span class="node-icon" :style="{ background: streamAccent(stream.id) }">
            {{ stream.app_name.charAt(0) }}
          </span>
          <div class="node-body">
            <strong class="stream-title">{{ stream.app_name }}</strong>
            <span class="node-sub">{{ streamSubtitle(stream) }}</span>
            <div
              class="routing-picker"
              :class="stream.direction === 'capture' ? 'capture' : 'playback'"
            >
              <span class="routing-label">Route to</span>
              <select
                class="routing-select"
                :value="stream.current_target ?? ''"
                @change="onTargetChange(stream.id, $event)"
              >
                <option value="" disabled>Select target</option>
                <option
                  v-for="target in targetsForStream(stream)"
                  :key="target.id"
                  :value="target.id"
                >
                  {{ targetLabel(target) }}
                </option>
              </select>
            </div>
          </div>
        </div>
      </section>

      <section class="column">
        <h3>Routing</h3>
        <p v-if="columns.routing.length === 0" class="empty">No virtual sinks</p>
        <div
          v-for="node in columns.routing"
          :key="node.id"
          :ref="(el) => setNodeRef(node.id, el as HTMLElement | null)"
          class="node"
        >
          <span
            class="node-icon routing"
            :style="{ borderColor: node.accent ?? 'var(--accent-purple)' }"
          >
            {{ node.label.charAt(0) }}
          </span>
          <div class="node-body">
            <NodeCardHeader
              v-if="deviceById(node.id)"
              :label="deviceById(node.id)!.label"
              editable
              :deletable="deviceById(node.id)!.system_name.startsWith('pipe-deck-')"
              @save="(name) => saveRename(deviceById(node.id)!, name)"
              @delete="removeVirtual(deviceById(node.id)!)"
            />
            <span class="node-sub">{{ node.subtitle }}</span>
            <div
              v-if="deviceById(node.id)"
              class="routing-picker playback"
            >
              <span class="routing-label">Route to</span>
              <select
                class="routing-select"
                :value="deviceById(node.id)!.current_target ?? ''"
                @change="onDeviceRouteChange(node.id, $event)"
              >
                <option value="" disabled>Select output</option>
                <option
                  v-for="target in targetsForVirtualSink(deviceById(node.id)!)"
                  :key="target.id"
                  :value="target.id"
                >
                  {{ target.label }}
                </option>
              </select>
            </div>
          </div>
        </div>
      </section>

      <section class="column">
        <h3>Outputs</h3>
        <div
          v-for="node in columns.outputs"
          :key="node.id"
          :ref="(el) => setNodeRef(node.id, el as HTMLElement | null)"
          class="node"
        >
          <span class="node-icon output">🔊</span>
          <div class="node-body">
            <NodeCardHeader
              v-if="deviceById(node.id)"
              :label="deviceById(node.id)!.label"
              editable
              :deletable="deviceById(node.id)!.kind === 'virtual' && deviceById(node.id)!.system_name.startsWith('pipe-deck-')"
              @save="(name) => saveRename(deviceById(node.id)!, name)"
              @delete="removeVirtual(deviceById(node.id)!)"
            />
            <span class="node-sub">{{ node.subtitle }}</span>
          </div>
        </div>
      </section>

      <section class="column">
        <h3>Inputs</h3>
        <div
          v-for="node in columns.inputs"
          :key="node.id"
          :ref="(el) => setNodeRef(node.id, el as HTMLElement | null)"
          class="node"
        >
          <span class="node-icon input">🎤</span>
          <div class="node-body">
            <NodeCardHeader
              v-if="deviceById(node.id)"
              :label="deviceById(node.id)!.label"
              editable
              :deletable="deviceById(node.id)!.kind === 'virtual' && deviceById(node.id)!.system_name.startsWith('pipe-deck-')"
              @save="(name) => saveRename(deviceById(node.id)!, name)"
              @delete="removeVirtual(deviceById(node.id)!)"
            />
            <span class="node-sub">{{ node.subtitle }}</span>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>
