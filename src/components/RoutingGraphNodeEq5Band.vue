<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";

const props = defineProps<{
  nodeId: string;
  eqSub: number;
  eqBass: number;
  eqMid: number;
  eqTreble: number;
  eqAir: number;
  outputGain: number;
}>();

const BANDS = [
  { key: "eqSub", label: "Sub", param: "eq_sub" },
  { key: "eqBass", label: "Bass", param: "eq_bass" },
  { key: "eqMid", label: "Mid", param: "eq_mid" },
  { key: "eqTreble", label: "Treble", param: "eq_treble" },
  { key: "eqAir", label: "Air", param: "eq_air" },
  { key: "outputGain", label: "Gain", param: "output_gain" },
] as const;

function valueFor(key: (typeof BANDS)[number]["key"]): number {
  return props[key];
}

async function onBandChange(param: (typeof BANDS)[number]["param"], event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  await invoke("update_processing_node_eq_params", {
    nodeId: props.nodeId,
    eqSub: param === "eq_sub" ? value : props.eqSub,
    eqBass: param === "eq_bass" ? value : props.eqBass,
    eqMid: param === "eq_mid" ? value : props.eqMid,
    eqTreble: param === "eq_treble" ? value : props.eqTreble,
    eqAir: param === "eq_air" ? value : props.eqAir,
    outputGain: param === "output_gain" ? value : props.outputGain,
  });
}
</script>

<template>
  <div class="routing-graph-node-eq5band nodrag">
    <div v-for="band in BANDS" :key="band.key" class="routing-graph-node-eq5band-row">
      <span class="routing-graph-node-eq5band-label">{{ band.label }}</span>
      <input
        type="range"
        class="routing-graph-node-eq5band-slider"
        min="-12"
        max="12"
        :value="valueFor(band.key)"
        :aria-label="`${band.label} gain`"
        @change="onBandChange(band.param, $event)"
      />
      <span class="routing-graph-node-eq5band-value">{{ valueFor(band.key) }}dB</span>
    </div>
  </div>
</template>
