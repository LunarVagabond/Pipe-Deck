<script setup lang="ts">
import { computed, inject, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useApplyResult } from "../stores/notices";
import { routingGraphActionsKey } from "../composables/routingGraphContext";
import { processingNodeNodeId } from "./routing-graph/nodeIds";

const props = defineProps<{
  nodeId: string;
  mixPercent: number;
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

/** Keeps a dragged slider's value on screen until the next graph push
 * confirms it — same reasoning as `RoutingGraphNodeDelay.vue`'s `pending`
 * ref. */
const pending = ref<number | null>(null);

const displayMixPercent = computed(() => pending.value ?? props.mixPercent);

function onMixInput(event: Event) {
  pending.value = Number((event.target as HTMLInputElement).value);
}

async function onMixChange(event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  pending.value = value;
  const response = await invoke<{ success: boolean; message?: string }>(
    "update_processing_node_reverb_params",
    {
      nodeId: props.nodeId,
      mixPercent: value,
    },
  );
  if (!response.success) {
    handleApplyResult(response, "");
  }
}

/** Resets Mix to fully dry (0%) — same command the slider already uses. */
async function onReset() {
  pending.value = null;
  const response = await invoke<{ success: boolean; message?: string }>(
    "update_processing_node_reverb_params",
    {
      nodeId: props.nodeId,
      mixPercent: 0,
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
    class="routing-graph-node-reverb nodrag"
    :class="{ 'is-bypassed': bypassed }"
  >
    <div class="routing-graph-node-reverb-actions">
      <button
        type="button"
        class="routing-graph-node-reverb-bypass"
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
        class="routing-graph-node-reverb-isolate"
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
    <div class="routing-graph-node-reverb-row">
      <span class="routing-graph-node-reverb-label">Mix</span>
      <input
        type="range"
        class="routing-graph-node-reverb-slider"
        min="0"
        max="100"
        :value="displayMixPercent"
        :disabled="bypassed"
        aria-label="Mix"
        @input="onMixInput"
        @change="onMixChange"
      />
      <span class="routing-graph-node-reverb-value"
        >{{ displayMixPercent }}%</span
      >
    </div>
  </div>
</template>
