<script setup lang="ts">
import { computed, inject, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useApplyResult } from "../stores/notices";
import { routingGraphActionsKey } from "../composables/routingGraphContext";
import { processingNodeNodeId } from "./routing-graph/nodeIds";

const props = defineProps<{
  nodeId: string;
  ceilingDb: number;
  bypassed: boolean;
}>();

const { handleApplyResult } = useApplyResult();
const actions = inject(routingGraphActionsKey, null);
const graphNodeId = computed(() => processingNodeNodeId(props.nodeId));
const isIsolated = computed(() => actions?.isEffectIsolated(graphNodeId.value) ?? false);

function onToggleIsolate() {
  void actions?.isolateEffectNode(graphNodeId.value);
}

const CONTROLS = [{ key: "ceilingDb", label: "Ceiling", param: "ceiling_db", min: -24, max: 0, unit: "dB" }] as const;

/** Keeps a dragged control's value on screen until the next graph push
 * confirms it — same reasoning as `RoutingGraphNodeDelay.vue`'s `pending`
 * ref: the live-apply push can race the node's own readiness right after
 * creation, and without local state here the slider would visibly snap back
 * to the last server-echoed value for that entire window. */
const pending = ref<Partial<Record<(typeof CONTROLS)[number]["key"], number>>>({});

function valueFor(key: (typeof CONTROLS)[number]["key"]): number {
  return pending.value[key] ?? props[key];
}

/** Updates the on-screen label on every drag tick — the actual live-apply
 * push stays gated behind `onControlChange`/`@change` so a drag doesn't spam
 * the backend one call per pixel of movement. */
function onControlInput(key: (typeof CONTROLS)[number]["key"], event: Event) {
  pending.value[key] = Number((event.target as HTMLInputElement).value);
}

async function onControlChange(event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  pending.value.ceilingDb = value;
  const response = await invoke<{ success: boolean; message?: string }>("update_processing_node_limiter_params", {
    nodeId: props.nodeId,
    ceilingDb: value,
  });
  if (!response.success) {
    handleApplyResult(response, "");
  }
}

/** Resets Ceiling to full scale (no clamp) — same command the slider
 * already uses. */
async function onReset() {
  pending.value = {};
  const response = await invoke<{ success: boolean; message?: string }>("update_processing_node_limiter_params", {
    nodeId: props.nodeId,
    ceilingDb: 0,
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
</script>

<template>
  <div class="routing-graph-node-limiter nodrag" :class="{ 'is-bypassed': bypassed }">
    <div class="routing-graph-node-limiter-actions">
      <button
        type="button"
        class="routing-graph-node-limiter-bypass"
        :class="{ active: bypassed }"
        :aria-pressed="bypassed"
        :title="bypassed ? 'Bypassed — passing through unprocessed' : 'Bypass — keep wiring, skip processing'"
        @click="onToggleBypass"
      >
        {{ bypassed ? "Bypassed" : "Bypass" }}
      </button>
      <button
        type="button"
        class="routing-graph-node-limiter-isolate"
        :class="{ active: isIsolated }"
        :aria-pressed="isIsolated"
        :title="isIsolated ? 'Isolated — click to restore other effects' : 'Isolate — bypass every other effect in this chain'"
        @click="onToggleIsolate"
      >
        {{ isIsolated ? "Isolated" : "Isolate" }}
      </button>
      <button
        type="button"
        class="routing-graph-node-limiter-reset"
        title="Reset ceiling to full scale (no clamp)"
        aria-label="Reset to defaults"
        @click="onReset"
      >
        ↺
      </button>
    </div>
    <div v-for="control in CONTROLS" :key="control.key" class="routing-graph-node-limiter-row">
      <span class="routing-graph-node-limiter-label">{{ control.label }}</span>
      <input
        type="range"
        class="routing-graph-node-limiter-slider"
        :min="control.min"
        :max="control.max"
        :value="valueFor(control.key)"
        :disabled="bypassed"
        :aria-label="control.label"
        @input="onControlInput(control.key, $event)"
        @change="onControlChange($event)"
      />
      <span class="routing-graph-node-limiter-value">{{ valueFor(control.key) }}{{ control.unit }}</span>
    </div>
  </div>
</template>
