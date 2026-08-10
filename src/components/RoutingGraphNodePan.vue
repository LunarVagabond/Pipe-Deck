<script setup lang="ts">
import { computed, inject, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useApplyResult } from "../stores/notices";
import { routingGraphActionsKey } from "../composables/routingGraphContext";
import { processingNodeNodeId } from "./routing-graph/nodeIds";

const props = defineProps<{
  nodeId: string;
  balancePercent: number;
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

const displayBalancePercent = computed(
  () => pending.value ?? props.balancePercent,
);

function onBalanceInput(event: Event) {
  pending.value = Number((event.target as HTMLInputElement).value);
}

async function onBalanceChange(event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  pending.value = value;
  const response = await invoke<{ success: boolean; message?: string }>(
    "update_processing_node_pan_params",
    {
      nodeId: props.nodeId,
      balancePercent: value,
    },
  );
  if (!response.success) {
    handleApplyResult(response, "");
  }
}

/** Resets Balance to 0 (center) — the neutral value, same convention as
 * Delay/HPF/Reverb. */
async function onReset() {
  pending.value = null;
  const response = await invoke<{ success: boolean; message?: string }>(
    "update_processing_node_pan_params",
    {
      nodeId: props.nodeId,
      balancePercent: 0,
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
    class="routing-graph-node-pan nodrag"
    :class="{ 'is-bypassed': bypassed }"
  >
    <div class="routing-graph-node-pan-actions">
      <button
        type="button"
        class="routing-graph-node-pan-bypass"
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
        class="routing-graph-node-pan-isolate"
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
    <div class="routing-graph-node-pan-row">
      <span class="routing-graph-node-pan-label">Balance</span>
      <input
        type="range"
        class="routing-graph-node-pan-slider"
        min="-100"
        max="100"
        :value="displayBalancePercent"
        :disabled="bypassed"
        aria-label="Balance"
        @input="onBalanceInput"
        @change="onBalanceChange"
      />
      <span class="routing-graph-node-pan-value"
        >{{ displayBalancePercent }}%</span
      >
    </div>
  </div>
</template>
