<script setup lang="ts">
import { computed, inject, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useApplyResult } from "../stores/notices";
import { routingGraphActionsKey } from "../composables/routingGraphContext";
import { processingNodeNodeId } from "./routing-graph/nodeIds";

const props = defineProps<{
  nodeId: string;
  eqSub: number;
  eqBass: number;
  eqMid: number;
  eqTreble: number;
  eqAir: number;
  outputGain: number;
  bypassed: boolean;
}>();

const { handleApplyResult } = useApplyResult();
const actions = inject(routingGraphActionsKey, null);
const graphNodeId = computed(() => processingNodeNodeId(props.nodeId));
const isIsolated = computed(() => actions?.isEffectIsolated(graphNodeId.value) ?? false);

function onToggleIsolate() {
  void actions?.isolateEffectNode(graphNodeId.value);
}

const BANDS = [
  { key: "eqSub", label: "Sub", param: "eq_sub" },
  { key: "eqBass", label: "Bass", param: "eq_bass" },
  { key: "eqMid", label: "Mid", param: "eq_mid" },
  { key: "eqTreble", label: "Treble", param: "eq_treble" },
  { key: "eqAir", label: "Air", param: "eq_air" },
  { key: "outputGain", label: "Gain", param: "output_gain" },
] as const;

/** Keeps a dragged band's value on screen until the next graph push confirms
 * it — the backend now always persists a drag even when it can't push it
 * live yet (see `update_processing_node_eq_params`'s soft-failure contract),
 * but that live-apply step can still race the node's own readiness right
 * after creation, and without local state here the slider would visibly
 * snap back to the last server-echoed value for that entire window. */
const pending = ref<Partial<Record<(typeof BANDS)[number]["key"], number>>>({});

function valueFor(key: (typeof BANDS)[number]["key"]): number {
  return pending.value[key] ?? props[key];
}

/** Updates the on-screen dB label on every drag tick — `@input` fires
 * continuously while dragging, unlike `@change`, which only fires once on
 * release. Purely cosmetic: the actual live-apply push stays gated behind
 * `onBandChange`/`@change` so a drag doesn't spam the backend one call per
 * pixel of movement. */
function onBandInput(key: (typeof BANDS)[number]["key"], event: Event) {
  pending.value[key] = Number((event.target as HTMLInputElement).value);
}

async function onBandChange(param: (typeof BANDS)[number]["param"], event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  const band = BANDS.find((entry) => entry.param === param);
  if (band) {
    pending.value[band.key] = value;
  }
  const response = await invoke<{ success: boolean; message?: string }>("update_processing_node_eq_params", {
    nodeId: props.nodeId,
    eqSub: param === "eq_sub" ? value : props.eqSub,
    eqBass: param === "eq_bass" ? value : props.eqBass,
    eqMid: param === "eq_mid" ? value : props.eqMid,
    eqTreble: param === "eq_treble" ? value : props.eqTreble,
    eqAir: param === "eq_air" ? value : props.eqAir,
    outputGain: param === "output_gain" ? value : props.outputGain,
  });
  if (!response.success) {
    handleApplyResult(response, "");
  }
}

/** Resets every band and the output trim to flat (0dB) — same command each
 * band slider already uses, just with every value zeroed at once rather
 * than one param changed at a time. */
async function onReset() {
  pending.value = {};
  const response = await invoke<{ success: boolean; message?: string }>("update_processing_node_eq_params", {
    nodeId: props.nodeId,
    eqSub: 0,
    eqBass: 0,
    eqMid: 0,
    eqTreble: 0,
    eqAir: 0,
    outputGain: 0,
  });
  if (!response.success) {
    handleApplyResult(response, "");
  }
}

/** Keeps every connection exactly as wired — only whether the signal comes
 * through processed or not changes. */
async function onToggleBypass() {
  const response = await invoke<{ success: boolean; message?: string }>("set_processing_node_bypassed", {
    nodeId: props.nodeId,
    bypassed: !props.bypassed,
  });
  if (!response.success) {
    handleApplyResult(response, "");
  }
}

/** Reset/warning render in the node's header (top-right, next to Delete —
 * see `RoutingGraphNode.vue`'s `toolbar-extra` slot usage), not in this
 * component's own template — `reset` stays defined here regardless, since
 * it needs this component's own `pending` ref to clear the optimistic
 * slider display the same way a real server-confirmed value would. */
defineExpose({ reset: onReset });
</script>

<template>
  <div class="routing-graph-node-eq5band nodrag" :class="{ 'is-bypassed': bypassed }">
    <div class="routing-graph-node-eq5band-actions">
      <button
        type="button"
        class="routing-graph-node-eq5band-bypass"
        :class="{ active: bypassed }"
        :aria-pressed="bypassed"
        :title="bypassed ? 'Bypassed — passing through unprocessed' : 'Bypass — keep wiring, skip processing'"
        @click="onToggleBypass"
      >
        {{ bypassed ? "Bypassed" : "Bypass" }}
      </button>
      <button
        type="button"
        class="routing-graph-node-eq5band-isolate"
        :class="{ active: isIsolated }"
        :aria-pressed="isIsolated"
        :title="isIsolated ? 'Isolated — click to restore other effects' : 'Isolate — bypass every other effect in this chain'"
        @click="onToggleIsolate"
      >
        {{ isIsolated ? "Isolated" : "Isolate" }}
      </button>
    </div>
    <div v-for="band in BANDS" :key="band.key" class="routing-graph-node-eq5band-row">
      <span class="routing-graph-node-eq5band-label">{{ band.label }}</span>
      <input
        type="range"
        class="routing-graph-node-eq5band-slider"
        min="-12"
        max="12"
        :value="valueFor(band.key)"
        :disabled="bypassed"
        :aria-label="`${band.label} gain`"
        @input="onBandInput(band.key, $event)"
        @change="onBandChange(band.param, $event)"
      />
      <span class="routing-graph-node-eq5band-value">{{ valueFor(band.key) }}dB</span>
    </div>
  </div>
</template>
