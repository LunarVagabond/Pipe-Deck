<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";

const props = defineProps<{
  nodeId: string;
  volumePercent: number;
  muted: boolean;
}>();

async function onVolumeChange(event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  await invoke("update_processing_node_volume", { nodeId: props.nodeId, volumePercent: value, muted: props.muted });
}

async function onToggleMute() {
  await invoke("update_processing_node_volume", {
    nodeId: props.nodeId,
    volumePercent: props.volumePercent,
    muted: !props.muted,
  });
}
</script>

<template>
  <div class="routing-graph-node-effects nodrag">
    <div class="routing-graph-node-effect-row routing-graph-node-effect-row--pinned">
      <button
        type="button"
        class="routing-graph-node-mute"
        :class="{ active: muted }"
        :aria-label="muted ? 'Unmute' : 'Mute'"
        @click="onToggleMute"
      >
        {{ muted ? "🔇" : "🔊" }}
      </button>
      <input
        type="range"
        class="routing-graph-node-volume"
        min="0"
        max="100"
        :value="volumePercent"
        aria-label="Fan-Out volume"
        @change="onVolumeChange"
      />
      <span class="routing-graph-node-volume-label">{{ volumePercent }}%</span>
    </div>
  </div>
</template>
