<script setup lang="ts">
import { inject, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { routingGraphActionsKey } from "../composables/routingGraphContext";
import { useApplyResult } from "../stores/notices";

interface MixerInputHandle {
  index: number;
  connectedId?: string;
}

const props = defineProps<{
  nodeId: string;
  /** Per-input gain, indexed the same way as `inputs` — see
   * `ProcessingNodeKind.mixer.input_gains_percent` (PD-032). */
  inputGainsPercent: number[];
  inputs: MixerInputHandle[];
}>();

const actions = inject(routingGraphActionsKey, null);
const { handleApplyResult } = useApplyResult();

/** Keeps the dragged value on screen until the next graph push confirms (or
 * overrides) it — without this, a slow round trip or a soft backend failure
 * (e.g. a stream-peer feed sink briefly unresolvable) visibly snaps the
 * slider back to the last server-echoed value mid-drag. */
const pendingGains = ref<Record<number, number>>({});

function labelFor(id: string): string {
  return actions?.labelForEntity(id) ?? id;
}

function gainFor(index: number): number {
  return pendingGains.value[index] ?? props.inputGainsPercent[index] ?? 100;
}

async function onGainChange(index: number, event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  pendingGains.value[index] = value;
  try {
    const response = await invoke<{ success: boolean; message?: string }>(
      "update_processing_node_input_gain",
      {
        nodeId: props.nodeId,
        portIndex: index,
        gainPercent: value,
        muted: false,
      },
    );
    if (!response.success) {
      handleApplyResult(response, "");
    }
  } catch (error) {
    handleApplyResult(
      {
        success: false,
        message: error instanceof Error ? error.message : String(error),
      },
      "",
    );
  }
}
</script>

<template>
  <div class="routing-graph-node-mixer-inputs nodrag">
    <p v-if="inputs.length === 0" class="routing-graph-node-mixer-inputs-empty">
      Drag a source in to mix it
    </p>
    <div
      v-for="input in inputs"
      :key="input.index"
      class="routing-graph-node-mixer-input-row"
    >
      <span class="routing-graph-node-mixer-input-label">
        {{ input.connectedId ? labelFor(input.connectedId) : "…" }}
      </span>
      <input
        type="range"
        class="routing-graph-node-mixer-input-gain"
        min="0"
        max="100"
        :value="gainFor(input.index)"
        :aria-label="`${input.connectedId ? labelFor(input.connectedId) : 'input'} gain`"
        @change="onGainChange(input.index, $event)"
      />
      <span class="routing-graph-node-mixer-input-gain-label"
        >{{ gainFor(input.index) }}%</span
      >
    </div>
  </div>
</template>
