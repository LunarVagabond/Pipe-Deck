<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import RoutingMatrix from "../components/RoutingMatrix.vue";
import ToggleSwitch from "../components/ToggleSwitch.vue";
import { useApplyResult } from "../stores/notices";
import { useAppConfig, useRuntimeGraph } from "../stores/runtimeGraph";
import { filterRuntimeGraph } from "../utils/filterGraph";

const { graph, loading, error, refresh } = useRuntimeGraph();
const { config } = useAppConfig();
const { handleApplyResult } = useApplyResult();

const showSystemStreams = ref(false);
const canUndo = ref(false);

async function refreshCanUndo() {
  try {
    canUndo.value = await invoke<boolean>("can_undo_routing");
  } catch {
    canUndo.value = false;
  }
}

watch(
  graph,
  () => {
    void refreshCanUndo();
  },
  { immediate: true },
);

watch(
  config,
  (value) => {
    showSystemStreams.value = value?.preferences?.show_system_streams ?? false;
  },
  { immediate: true },
);

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

async function onToggleSystemStreams(next: boolean) {
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

async function undoRouting() {
  if (!canUndo.value) return;

  try {
    const result = await invoke<{ success: boolean; message?: string }>("undo_last_routing");
    handleApplyResult(result, "Routing change undone");
    await refreshCanUndo();
  } catch (err) {
    handleApplyResult(
      { success: false, message: err instanceof Error ? err.message : String(err) },
      "",
    );
  }
}
</script>

<template>
  <div class="routing-view">
    <header class="routing-header">
      <div>
        <p class="eyebrow">Topology and connections</p>
        <h1>Routing</h1>
      </div>
      <div class="routing-actions">
        <div class="header-toggle">
          <span class="toggle-row-label">Show system streams</span>
          <ToggleSwitch
            :model-value="showSystemStreams"
            :show-state-labels="false"
            @update:model-value="onToggleSystemStreams"
          />
        </div>
        <button type="button" :disabled="!canUndo" @click="undoRouting">Undo</button>
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

    <RoutingMatrix v-else :graph="displayGraph" />
  </div>
</template>
