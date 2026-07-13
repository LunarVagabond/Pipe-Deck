<script setup lang="ts">
import { computed, inject, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import ToggleSwitch from "../components/ToggleSwitch.vue";
import { navigateKey } from "../composables/navigation";
import { useAppConfig, useRuntimeGraph } from "../stores/runtimeGraph";
import { useRuleDraft } from "../stores/ruleDraft";
import { filterRuntimeGraph } from "../utils/filterGraph";
import { deviceColumn } from "../utils/routingLayout";
import { filterRecentlySeen, recentEntryAgo, recentEntryLabel } from "../utils/recentStreams";
import type { RecentStreamIdentity } from "../types/graph";

const { graph, loading, error, refresh } = useRuntimeGraph();
const { config } = useAppConfig();
const { requestRuleForIdentity } = useRuleDraft();
const navigate = inject(navigateKey);

const recentlySeenEntries = computed(() => filterRecentlySeen(graph.value.recent_stream_identities));

function createRuleForIdentity(entry: RecentStreamIdentity) {
  requestRuleForIdentity(entry);
  navigate?.("rules");
}

const showSystemStreams = ref(false);

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

function openRoutingGraph() {
  navigate?.("routing");
}
</script>

<template>
  <div class="dashboard">
    <header class="dashboard-header view-header">
      <div>
        <p class="eyebrow">Live PipeWire overview</p>
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

      <section v-if="recentlySeenEntries.length > 0" class="dashboard-section">
        <div class="dashboard-section-header">
          <h2>Recently seen</h2>
          <button type="button" class="link-btn" @click="navigate?.('rules')">
            Manage rules →
          </button>
        </div>
        <p class="dashboard-section-hint">
          Apps that briefly appeared in the last hour but aren't active right now.
        </p>
        <ul class="recently-seen-list">
          <li v-for="(entry, index) in recentlySeenEntries" :key="`${entry.app_name}-${index}`">
            <div class="recently-seen-info">
              <strong>{{ recentEntryLabel(entry) }}</strong>
              <span class="recently-seen-meta">
                {{ entry.direction === "capture" ? "Capture" : "Playback" }} · {{ recentEntryAgo(entry) }}
              </span>
            </div>
            <button
              type="button"
              class="recently-seen-create-btn"
              @click="createRuleForIdentity(entry)"
            >
              Create rule
            </button>
          </li>
        </ul>
      </section>

      <section class="dashboard-section">
        <div class="dashboard-section-header">
          <h2>Devices</h2>
          <button type="button" class="link-btn" @click="openRoutingGraph">
            Open full routing graph →
          </button>
        </div>
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
