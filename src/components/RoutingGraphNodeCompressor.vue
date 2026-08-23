<script setup lang="ts">
import { computed, inject, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useApplyResult } from "../stores/notices";
import { routingGraphActionsKey } from "../composables/routingGraphContext";
import { processingNodeNodeId } from "./routing-graph/nodeIds";

const props = defineProps<{
  nodeId: string;
  thresholdDb: number;
  ratioX10: number;
  attackMs: number;
  releaseMs: number;
  makeupGainDb: number;
  bypassed: boolean;
}>();

const { handleApplyResult } = useApplyResult();
const actions = inject(routingGraphActionsKey, null);
const graphNodeId = computed(() => processingNodeNodeId(props.nodeId));
const isIsolated = computed(
  () => actions?.isEffectIsolated(graphNodeId.value) ?? false,
);

function onToggleIsolate() {
  void actions?.isolateEffectNode(graphNodeId.value);
}

const CONTROLS = [
  {
    key: "thresholdDb",
    label: "Threshold",
    param: "threshold_db",
    min: -60,
    max: 0,
    unit: "dB",
  },
  {
    key: "ratioX10",
    label: "Ratio",
    param: "ratio_x10",
    min: 10,
    max: 200,
    // Fixed-point (ratio_x10 / 10) — displayed as e.g. "4.0:1".
    format: (value: number) => `${(value / 10).toFixed(1)}:1`,
  },
  {
    key: "attackMs",
    label: "Attack",
    param: "attack_ms",
    min: 1,
    max: 200,
    unit: "ms",
  },
  {
    key: "releaseMs",
    label: "Release",
    param: "release_ms",
    min: 10,
    max: 2000,
    unit: "ms",
  },
  {
    key: "makeupGainDb",
    label: "Makeup Gain",
    param: "makeup_gain_db",
    min: 0,
    max: 24,
    unit: "dB",
  },
] as const;

/** Keeps a dragged control's value on screen until the next graph push
 * confirms it — same reasoning as `RoutingGraphNodeDelay.vue`'s `pending`
 * ref. */
const pending = ref<Partial<Record<(typeof CONTROLS)[number]["key"], number>>>(
  {},
);

function valueFor(key: (typeof CONTROLS)[number]["key"]): number {
  return pending.value[key] ?? props[key];
}

function displayFor(control: (typeof CONTROLS)[number]): string {
  const value = valueFor(control.key);
  return "format" in control
    ? control.format(value)
    : `${value}${control.unit}`;
}

function onControlInput(key: (typeof CONTROLS)[number]["key"], event: Event) {
  pending.value[key] = Number((event.target as HTMLInputElement).value);
}

async function onControlChange(
  param: (typeof CONTROLS)[number]["param"],
  event: Event,
) {
  const value = Number((event.target as HTMLInputElement).value);
  const control = CONTROLS.find((entry) => entry.param === param);
  if (control) {
    pending.value[control.key] = value;
  }
  const response = await invoke<{ success: boolean; message?: string }>(
    "update_processing_node_compressor_params",
    {
      nodeId: props.nodeId,
      thresholdDb: param === "threshold_db" ? value : props.thresholdDb,
      ratioX10: param === "ratio_x10" ? value : props.ratioX10,
      attackMs: param === "attack_ms" ? value : props.attackMs,
      releaseMs: param === "release_ms" ? value : props.releaseMs,
      makeupGainDb: param === "makeup_gain_db" ? value : props.makeupGainDb,
    },
  );
  if (!response.success) {
    handleApplyResult(response, "");
  }
}

/** Resets to a mild, neutral-ish starting point (-18dB/4:1/10ms/150ms/0dB)
 * — a compressor has no true "off" numeric state the way Delay's all-zeros
 * or Pan's center does (a real ratio floor of 1:1 combined with these
 * defaults is already close to inaudible on typical material). */
async function onReset() {
  pending.value = {};
  const response = await invoke<{ success: boolean; message?: string }>(
    "update_processing_node_compressor_params",
    {
      nodeId: props.nodeId,
      thresholdDb: -18,
      ratioX10: 40,
      attackMs: 10,
      releaseMs: 150,
      makeupGainDb: 0,
    },
  );
  if (!response.success) {
    handleApplyResult(response, "");
  }
}

async function onToggleBypass() {
  const response = await invoke<{ success: boolean; message?: string }>(
    "set_processing_node_bypassed",
    {
      nodeId: props.nodeId,
      bypassed: !props.bypassed,
    },
  );
  if (!response.success) {
    handleApplyResult(response, "");
  }
}

defineExpose({ reset: onReset });
</script>

<template>
  <div
    class="routing-graph-node-compressor nodrag"
    :class="{ 'is-bypassed': bypassed }"
  >
    <div class="routing-graph-node-compressor-actions">
      <button
        type="button"
        class="routing-graph-node-compressor-bypass"
        :class="{ active: bypassed }"
        :aria-pressed="bypassed"
        :title="
          bypassed
            ? 'Bypassed — passing through unprocessed'
            : 'Bypass — keep wiring, skip processing'
        "
        @click="onToggleBypass"
      >
        {{ bypassed ? "Bypassed" : "Bypass" }}
      </button>
      <button
        type="button"
        class="routing-graph-node-compressor-isolate"
        :class="{ active: isIsolated }"
        :aria-pressed="isIsolated"
        :title="
          isIsolated
            ? 'Isolated — click to restore other effects'
            : 'Isolate — bypass every other effect in this chain'
        "
        @click="onToggleIsolate"
      >
        {{ isIsolated ? "Isolated" : "Isolate" }}
      </button>
    </div>
    <div
      v-for="control in CONTROLS"
      :key="control.key"
      class="routing-graph-node-compressor-row"
    >
      <span class="routing-graph-node-compressor-label">{{
        control.label
      }}</span>
      <input
        type="range"
        class="routing-graph-node-compressor-slider"
        :min="control.min"
        :max="control.max"
        :value="valueFor(control.key)"
        :disabled="bypassed"
        :aria-label="control.label"
        @input="onControlInput(control.key, $event)"
        @change="onControlChange(control.param, $event)"
      />
      <span class="routing-graph-node-compressor-value">{{
        displayFor(control)
      }}</span>
    </div>
  </div>
</template>
