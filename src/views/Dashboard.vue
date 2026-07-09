<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import MixerStrip from "../components/MixerStrip.vue";
import RoutingMatrix from "../components/RoutingMatrix.vue";
import { useAppConfig, useRuntimeGraph } from "../stores/runtimeGraph";
import { filterRuntimeGraph } from "../utils/filterGraph";

const { graph, loading, error, refresh } = useRuntimeGraph();
const { config } = useAppConfig();

const showSystemStreams = ref(false);

watch(
  config,
  (value) => {
    showSystemStreams.value = value?.preferences?.show_system_streams ?? false;
  },
  { immediate: true },
);

const profileName = computed(() => {
  const active = config.value?.active_profile;
  const entry = config.value?.profile_index.find((p) => p.id === active);
  return entry?.name ?? active ?? "Default";
});

const displayGraph = computed(() =>
  filterRuntimeGraph(graph.value, showSystemStreams.value),
);

const isMockData = computed(() => graph.value.data_source === "mock");
const isEmpty = computed(
  () =>
    !loading.value &&
    !error.value &&
    displayGraph.value.devices.length === 0 &&
    displayGraph.value.streams.length === 0,
);

async function onToggleSystemStreams(event: Event) {
  const next = (event.target as HTMLInputElement).checked;
  const previous = showSystemStreams.value;
  showSystemStreams.value = next;

  try {
    await invoke("set_show_system_streams", { show: next });
    if (config.value) {
      config.value = {
        ...config.value,
        preferences: {
          ...config.value.preferences,
          show_system_streams: next,
        },
      };
    }
  } catch {
    showSystemStreams.value = previous;
  }
}
</script>

<template>
  <div class="dashboard">
    <header class="header">
      <div>
        <p class="eyebrow">Linux Audio Control Center</p>
        <h1>Dashboard</h1>
      </div>
      <div class="header-actions">
        <label class="toggle-switch">
          <span class="toggle-label">Show system streams</span>
          <input
            type="checkbox"
            class="toggle-input"
            :checked="showSystemStreams"
            @change="onToggleSystemStreams"
          />
          <span class="toggle-track" aria-hidden="true">
            <span class="toggle-thumb" />
          </span>
        </label>
        <span class="profile-pill">{{ profileName }}</span>
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
    <p v-else-if="isEmpty" class="status">
      No PipeWire audio devices or application streams detected.
    </p>

    <template v-else>
      <RoutingMatrix :graph="displayGraph" />
      <MixerStrip :devices="displayGraph.devices" />
    </template>
  </div>
</template>
