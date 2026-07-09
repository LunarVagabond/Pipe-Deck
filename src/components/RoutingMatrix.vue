<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import type { RuntimeGraph } from "../types/graph";
import {
  deviceColumn,
  deviceSubtitle,
  linkColor,
  streamAccent,
  type MatrixNode,
} from "../utils/routingLayout";
import type { Device } from "../types/graph";

const props = defineProps<{
  graph: RuntimeGraph;
}>();

const matrixRef = ref<HTMLElement | null>(null);
const nodeRefs = ref<Record<string, HTMLElement | null>>({});

interface LinePath {
  id: string;
  d: string;
  color: string;
}

const lines = ref<LinePath[]>([]);

const columns = computed(() => {
  const apps: MatrixNode[] = props.graph.streams.map((stream) => ({
      id: stream.id,
      label: stream.app_name,
      column: "applications" as const,
      accent: streamAccent(stream.id),
      subtitle: stream.is_system
        ? "System stream"
        : stream.direction === "capture"
          ? "Capture stream"
          : "Playback stream",
    }));

  const routing: MatrixNode[] = [];
  const outputs: MatrixNode[] = [];
  const inputs: MatrixNode[] = [];

  for (const device of props.graph.devices) {
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
  for (const link of props.graph.links) {
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

watch(() => props.graph, () => updateLines(), { deep: true });

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
          v-for="node in columns.apps"
          :key="node.id"
          :ref="(el) => setNodeRef(node.id, el as HTMLElement | null)"
          class="node"
        >
          <span class="node-icon" :style="{ background: node.accent }">
            {{ node.label.charAt(0) }}
          </span>
          <div>
            <strong>{{ node.label }}</strong>
            <span v-if="node.subtitle" class="node-sub">{{ node.subtitle }}</span>
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
          <div>
            <strong>{{ node.label }}</strong>
            <span class="node-sub">{{ node.subtitle }}</span>
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
          <div>
            <strong>{{ node.label }}</strong>
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
          <div>
            <strong>{{ node.label }}</strong>
            <span class="node-sub">{{ node.subtitle }}</span>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>
