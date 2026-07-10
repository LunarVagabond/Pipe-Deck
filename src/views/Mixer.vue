<script setup lang="ts">
import { computed } from "vue";
import MixerStrip from "../components/MixerStrip.vue";
import { useRuntimeGraph } from "../stores/runtimeGraph";

const { graph, loading, error, refresh } = useRuntimeGraph();

const isMockData = computed(() => graph.value.data_source === "mock");

const hasMixerChannels = computed(
  () =>
    graph.value.devices.some((device) => device.volume_percent !== undefined) ||
    graph.value.streams.some(
      (stream) => !stream.is_system && stream.volume_percent !== undefined,
    ),
);

const mixerStreams = computed(() =>
  graph.value.streams.filter((stream) => !stream.is_system),
);

const isEmpty = computed(
  () => !loading.value && !error.value && !hasMixerChannels.value,
);
</script>

<template>
  <div class="mixer-view">
    <header class="mixer-header">
      <div>
        <p class="eyebrow">Levels and mute</p>
        <h1>Mixer</h1>
      </div>
      <div class="mixer-actions">
        <button type="button" @click="refresh">Refresh</button>
      </div>
    </header>

    <p v-if="isMockData" class="notice-banner mock">
      {{ graph.notice ?? "Showing sample data (PIPE_DECK_USE_MOCK=1)." }}
    </p>
    <p v-else-if="graph.notice" class="notice-banner warn">
      {{ graph.notice }}
    </p>

    <p v-if="loading" class="status">Loading runtime graph…</p>
    <p v-else-if="error" class="status error">{{ error }}</p>
    <p v-else-if="isEmpty" class="status">No mixer channels detected.</p>

    <MixerStrip v-else :devices="graph.devices" :streams="mixerStreams" />
  </div>
</template>
