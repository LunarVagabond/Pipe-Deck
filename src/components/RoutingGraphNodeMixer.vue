<script setup lang="ts">
import { inject } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { routingGraphActionsKey } from "../composables/routingGraphContext";

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

function labelFor(id: string): string {
  return actions?.labelForEntity(id) ?? id;
}

function gainFor(index: number): number {
  return props.inputGainsPercent[index] ?? 100;
}

async function onGainChange(index: number, event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  await invoke("update_processing_node_input_gain", {
    nodeId: props.nodeId,
    portIndex: index,
    gainPercent: value,
    muted: false,
  });
}
</script>

<template>
  <div class="routing-graph-node-mixer-inputs nodrag">
    <p v-if="inputs.length === 0" class="routing-graph-node-mixer-inputs-empty">Drag a source in to mix it</p>
    <div v-for="input in inputs" :key="input.index" class="routing-graph-node-mixer-input-row">
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
      <span class="routing-graph-node-mixer-input-gain-label">{{ gainFor(input.index) }}%</span>
    </div>
  </div>
</template>
