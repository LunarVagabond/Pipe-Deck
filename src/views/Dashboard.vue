<script setup lang="ts">
import { computed, inject, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import StreamTargetPicker from "../components/StreamTargetPicker.vue";
import ToggleSwitch from "../components/ToggleSwitch.vue";
import { navigateKey } from "../composables/navigation";
import { useAppConfig, useRuntimeGraph } from "../stores/runtimeGraph";
import { filterRuntimeGraph } from "../utils/filterGraph";
import { deviceColumn, targetLabel } from "../utils/routingLayout";
import type { Device, Stream } from "../types/graph";

const { graph, loading, error, refresh } = useRuntimeGraph();
const { config } = useAppConfig();
const navigate = inject(navigateKey);

const showSystemStreams = ref(false);
const canUndo = ref(false);

watch(
  config,
  (value) => {
    showSystemStreams.value = value?.preferences?.show_system_streams ?? false;
  },
  { immediate: true },
);

watch(
  graph,
  () => {
    void refreshCanUndo();
  },
  { immediate: true },
);

const displayGraph = computed(() =>
  filterRuntimeGraph(graph.value, showSystemStreams.value),
);

const profileName = computed(() => {
  const active = config.value?.active_profile;
  const entry = config.value?.profile_index.find((p) => p.id === active);
  return entry?.name ?? active ?? "Default";
});

const isMockData = computed(() => graph.value.data_source === "mock");

const playbackStreams = computed(() =>
  displayGraph.value.streams.filter((stream) => stream.direction === "playback"),
);

const captureStreams = computed(() =>
  displayGraph.value.streams.filter((stream) => stream.direction === "capture"),
);

const routableStreams = computed(() => [...playbackStreams.value, ...captureStreams.value]);

const virtualDeviceCount = computed(
  () => displayGraph.value.devices.filter((device) => device.kind === "virtual").length,
);

const outputsInUse = computed(() => {
  const ids = new Set<string>();
  for (const stream of displayGraph.value.streams) {
    if (stream.current_target) ids.add(stream.current_target);
  }
  for (const device of displayGraph.value.devices) {
    if (device.current_target) ids.add(device.current_target);
    for (const target of device.current_targets ?? []) {
      ids.add(target);
    }
  }
  return ids.size;
});

function deviceById(id?: string): Device | undefined {
  if (!id) return undefined;
  return displayGraph.value.devices.find((device) => device.id === id);
}

function streamTargetLabel(stream: Stream): string {
  const device = deviceById(stream.current_target);
  return device ? targetLabel(device) : "Not routed";
}

async function refreshCanUndo() {
  try {
    canUndo.value = await invoke<boolean>("can_undo_routing");
  } catch {
    canUndo.value = false;
  }
}

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
    await invoke("undo_last_routing");
    await refreshCanUndo();
  } catch {
    // notices handled by matrix/graph views
  }
}

function openRoutingGraph() {
  navigate?.("routing");
}
</script>

<template>
  <div class="dashboard">
    <header class="dashboard-header view-header">
      <div>
        <p class="eyebrow">Live PipeWire overview</p>
        <h1>Dashboard</h1>
      </div>
      <div class="dashboard-actions view-actions">
        <div class="header-toggle">
          <span class="toggle-row-label">Show system streams</span>
          <ToggleSwitch
            :model-value="showSystemStreams"
            :show-state-labels="false"
            @update:model-value="onToggleSystemStreams"
          />
        </div>
        <span class="profile-pill">{{ profileName }}</span>
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

    <template v-else>
      <div class="dashboard-stats">
        <article class="stat-card">
          <span class="stat-label">Playback apps</span>
          <strong class="stat-value">{{ playbackStreams.length }}</strong>
        </article>
        <article class="stat-card">
          <span class="stat-label">Capture apps</span>
          <strong class="stat-value">{{ captureStreams.length }}</strong>
        </article>
        <article class="stat-card">
          <span class="stat-label">Outputs in use</span>
          <strong class="stat-value">{{ outputsInUse }}</strong>
        </article>
        <article class="stat-card">
          <span class="stat-label">Virtual devices</span>
          <strong class="stat-value">{{ virtualDeviceCount }}</strong>
        </article>
      </div>

      <section class="dashboard-section">
        <div class="dashboard-section-header">
          <h2>Quick routing</h2>
          <button type="button" class="link-btn" @click="openRoutingGraph">
            Open full routing graph →
          </button>
        </div>
        <p v-if="routableStreams.length === 0" class="empty">
          No application streams detected. Launch an app that plays or records audio.
        </p>
        <div v-else class="dashboard-stream-table">
          <div
            v-for="stream in routableStreams"
            :key="stream.id"
            class="dashboard-stream-row"
          >
            <div class="stream-row-app">
              <strong>{{ stream.app_name }}</strong>
              <span
                class="direction-badge"
                :class="stream.direction === 'capture' ? 'capture' : 'playback'"
              >
                {{ stream.direction === "capture" ? "Capture" : "Playback" }}
              </span>
            </div>
            <span class="target-cell">{{ streamTargetLabel(stream) }}</span>
            <div class="compact-route-cell">
              <StreamTargetPicker
                :stream="stream"
                :devices="displayGraph.devices"
                compact
              />
            </div>
          </div>
        </div>
      </section>

      <section class="dashboard-section">
        <h2>Devices</h2>
        <div class="dashboard-device-summary">
          <article
            v-for="device in displayGraph.devices.filter((d) => deviceColumn(d))"
            :key="device.id"
            class="stat-card device-summary-card"
          >
            <span class="stat-label">{{ device.label }}</span>
            <span class="node-sub">{{ device.kind }} · {{ device.direction }}</span>
          </article>
        </div>
      </section>
    </template>
  </div>
</template>
