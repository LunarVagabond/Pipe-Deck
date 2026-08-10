<script setup lang="ts">
import { computed, inject, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useApplyResult } from "../stores/notices";
import { routingGraphActionsKey } from "../composables/routingGraphContext";
import { processingNodeNodeId } from "./routing-graph/nodeIds";

const props = defineProps<{
  nodeId: string;
  freqHz: number;
  resonanceX10: number;
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
    key: "freqHz",
    label: "Frequency",
    param: "freq_hz",
    min: 20,
    max: 2000,
    unit: "Hz",
  },
  {
    key: "resonanceX10",
    label: "Resonance (Q)",
    param: "resonance_x10",
    min: 1,
    max: 100,
    unit: "",
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

/** Resonance is stored/transmitted as `Q * 10` (an integer, see
 * `ProcessingNodeKind::Hpf`'s Rust doc comment) but displayed as the real Q
 * value (e.g. `70` on the wire reads as `7.0` here) — Frequency has no such
 * scaling. */
function displayValueFor(key: (typeof CONTROLS)[number]["key"]): string {
  const value = valueFor(key);
  return key === "resonanceX10" ? (value / 10).toFixed(1) : String(value);
}

/** Updates the on-screen label on every drag tick — the actual live-apply
 * push stays gated behind `onControlChange`/`@change` so a drag doesn't spam
 * the backend one call per pixel of movement. */
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
    "update_processing_node_hpf_params",
    {
      nodeId: props.nodeId,
      freqHz: param === "freq_hz" ? value : props.freqHz,
      resonanceX10: param === "resonance_x10" ? value : props.resonanceX10,
    },
  );
  if (!response.success) {
    handleApplyResult(response, "");
  }
}

/** Resets Frequency/Resonance to their defaults (near-transparent, no
 * resonant peak) — same command each slider already uses, both values reset
 * at once. */
async function onReset() {
  pending.value = {};
  const response = await invoke<{ success: boolean; message?: string }>(
    "update_processing_node_hpf_params",
    {
      nodeId: props.nodeId,
      freqHz: 20,
      resonanceX10: 7,
    },
  );
  if (!response.success) {
    handleApplyResult(response, "");
  }
}

/** Keeps every connection exactly as wired — only whether the signal comes
 * through processed or not changes. */
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

/** Reset renders in the node's header (top-right, next to Delete — see
 * `RoutingGraphNode.vue`'s `toolbar-extra` slot usage), not in this
 * component's own template — `reset` stays defined here regardless, since
 * it needs this component's own `pending` ref to clear the optimistic
 * slider display the same way a real server-confirmed value would. */
defineExpose({ reset: onReset });
</script>

<template>
  <div
    class="routing-graph-node-hpf nodrag"
    :class="{ 'is-bypassed': bypassed }"
  >
    <div class="routing-graph-node-hpf-actions">
      <button
        type="button"
        class="routing-graph-node-hpf-bypass"
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
        class="routing-graph-node-hpf-isolate"
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
      class="routing-graph-node-hpf-row"
    >
      <span class="routing-graph-node-hpf-label">{{ control.label }}</span>
      <input
        type="range"
        class="routing-graph-node-hpf-slider"
        :min="control.min"
        :max="control.max"
        :value="valueFor(control.key)"
        :disabled="bypassed"
        :aria-label="`${control.label}`"
        @input="onControlInput(control.key, $event)"
        @change="onControlChange(control.param, $event)"
      />
      <span class="routing-graph-node-hpf-value"
        >{{ displayValueFor(control.key) }}{{ control.unit }}</span
      >
    </div>
  </div>
</template>
