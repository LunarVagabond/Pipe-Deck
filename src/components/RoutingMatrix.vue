<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import RouteExplanationPanel from "./RouteExplanationPanel.vue";
import NodeCardHeader from "./NodeCardHeader.vue";
import SinkRoutePicker from "./SinkRoutePicker.vue";
import { useApplyResult } from "../stores/notices";
import { useConfirm } from "../stores/confirm";
import type { Device, RuntimeGraph, Stream } from "../types/graph";
import {
  deviceColumn,
  deviceSubtitle,
  isMultiSink,
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
const { confirm } = useConfirm();
const matrixRef = ref<HTMLElement | null>(null);
const nodeRefs = ref<Record<string, HTMLElement | null>>({});

interface LinePath {
  id: string;
  d: string;
  color: string;
  markerId: string;
}

function markerIdForColor(color: string): string {
  return `routing-arrow-${color.replace("#", "")}`;
}

const arrowColors = computed(() => [...new Set(lines.value.map((line) => line.color))]);

function streamForLink(link: { source_id: string; target_id: string }) {
  return graph.streams.find((stream) => stream.id === link.source_id);
}

function linkAudioFlow(link: { source_id: string; target_id: string }) {
  const stream = streamForLink(link);
  if (stream?.direction === "capture") {
    // Capture reads from the input device: mic → app.
    return {
      fromId: link.target_id,
      toId: link.source_id,
    };
  }

  return {
    fromId: link.source_id,
    toId: link.target_id,
  };
}

function nodeAnchor(
  el: HTMLElement,
  side: "left" | "right",
  inset = 0,
): { x: number; y: number } {
  const point = nodeCenter(el, side);
  point.x += side === "left" ? inset : -inset;
  return point;
}

function connectionSides(fromId: string, toId: string) {
  const fromEl = nodeRefs.value[fromId];
  const toEl = nodeRefs.value[toId];
  if (!fromEl || !toEl) {
    return null;
  }

  const fromLeft = nodeCenter(fromEl, "left").x;
  const fromRight = nodeCenter(fromEl, "right").x;
  const toLeft = nodeCenter(toEl, "left").x;
  const toRight = nodeCenter(toEl, "right").x;
  const fromCenterX = (fromLeft + fromRight) / 2;
  const toCenterX = (toLeft + toRight) / 2;

  if (toCenterX >= fromCenterX) {
    return {
      fromSide: "right" as const,
      toSide: "left" as const,
    };
  }

  return {
    fromSide: "left" as const,
    toSide: "right" as const,
  };
}

function buildPath(
  from: { x: number; y: number },
  to: { x: number; y: number },
  fromSide: "left" | "right",
  toSide: "left" | "right",
): string {
  const span = Math.abs(to.x - from.x);
  const dx = Math.max(span * 0.45, 40);
  const c1x = fromSide === "right" ? from.x + dx : from.x - dx;
  const c2x = toSide === "left" ? to.x - dx : to.x + dx;
  return `M ${from.x} ${from.y} C ${c1x} ${from.y}, ${c2x} ${to.y}, ${to.x} ${to.y}`;
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

function sinksForStream(stream: Stream) {
  return graph.devices.filter((device) => {
    if (device.system_name.startsWith("pipe-deck-feed-")) return false;
    if (stream.direction === "playback") {
      if (device.kind === "virtual" && device.direction === "output") {
        return true;
      }
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

async function updateLines() {
  await nextTick();

  const nextLines: LinePath[] = [];
  for (const link of graph.links) {
    const { fromId, toId } = linkAudioFlow(link);
    const sides = connectionSides(fromId, toId);
    if (!sides) continue;

    const fromEl = nodeRefs.value[fromId];
    const toEl = nodeRefs.value[toId];
    if (!fromEl || !toEl) continue;

    const from = nodeAnchor(fromEl, sides.fromSide, 3);
    const to = nodeAnchor(toEl, sides.toSide, 3);

    const color = linkColor(link.source_id, link.target_id);
    nextLines.push({
      id: link.id,
      d: buildPath(from, to, sides.fromSide, sides.toSide),
      color,
      markerId: markerIdForColor(color),
    });
  }

  lines.value = nextLines;
}

async function onStreamTargetChange(streamId: string, event: Event) {
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
  const confirmed = await confirm(`Delete virtual device "${device.label}"?`, {
    title: "Delete virtual device",
    confirmLabel: "Delete",
    cancelLabel: "Cancel",
  });
  if (!confirmed) {
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
      <defs>
        <marker
          v-for="color in arrowColors"
          :id="markerIdForColor(color)"
          :key="color"
          viewBox="0 0 12 12"
          refX="11"
          refY="6"
          markerWidth="12"
          markerHeight="12"
          orient="auto"
          markerUnits="userSpaceOnUse"
        >
          <path
            d="M0,1 L11,6 L0,11 Z"
            :fill="color"
            fill-opacity="0.95"
            :stroke="color"
            stroke-width="0.75"
            stroke-linejoin="round"
          />
        </marker>
      </defs>
      <path
        v-for="line in lines"
        :key="line.id"
        :d="line.d"
        :stroke="line.color"
        stroke-width="2.5"
        fill="none"
        stroke-opacity="0.9"
        stroke-linecap="round"
        :marker-end="`url(#${line.markerId})`"
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
              <span class="routing-label">
                {{ stream.direction === "capture" ? "Record from" : "Route to" }}
              </span>
              <select
                class="routing-select"
                :data-stream-route-select="stream.id"
                :value="stream.current_target ?? ''"
                @change="onStreamTargetChange(stream.id, $event)"
              >
                <option value="" disabled>Select target</option>
                <option
                  v-for="target in sinksForStream(stream)"
                  :key="target.id"
                  :value="target.id"
                >
                  {{ targetLabel(target) }}
                </option>
              </select>
            </div>
            <RouteExplanationPanel :stream="stream" :devices="graph.devices" />
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
              <span class="routing-label">{{ isMultiSink(deviceById(node.id)!) ? "Outputs" : "Route to" }}</span>
              <SinkRoutePicker
                v-if="isMultiSink(deviceById(node.id)!)"
                :sink="deviceById(node.id)!"
                :targets="targetsForVirtualSink(deviceById(node.id)!)"
              />
              <select
                v-else
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
