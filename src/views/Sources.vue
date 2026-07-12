<script setup lang="ts">
import { computed, inject, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import NodeCardHeader from "../components/NodeCardHeader.vue";
import StreamTargetPicker from "../components/StreamTargetPicker.vue";
import ToggleSwitch from "../components/ToggleSwitch.vue";
import { navigateKey } from "../composables/navigation";
import { useApplyResult } from "../stores/notices";
import { useConfirm } from "../stores/confirm";
import { useAppConfig, useRuntimeGraph } from "../stores/runtimeGraph";
import type { Device, RecentStreamIdentity } from "../types/graph";
import { filterRuntimeGraph } from "../utils/filterGraph";
import { deviceSubtitle } from "../utils/routingLayout";

const { graph, loading, error, refresh } = useRuntimeGraph();
const { config } = useAppConfig();
const { handleApplyResult } = useApplyResult();
const { confirm } = useConfirm();
const navigate = inject(navigateKey, null);

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

const isMockData = computed(() => graph.value.data_source === "mock");

const captureStreams = computed(() =>
  displayGraph.value.streams.filter((stream) => stream.direction === "capture"),
);

const playbackStreams = computed(() =>
  displayGraph.value.streams.filter((stream) => stream.direction === "playback"),
);

const inputDevices = computed(() =>
  displayGraph.value.devices.filter(
    (device) =>
      (device.direction === "input" || device.direction === "duplex") &&
      !device.system_name.startsWith("pipe-deck-feed-"),
  ),
);

const recentCaptureIdentities = computed(() =>
  (graph.value.recent_stream_identities ?? []).filter(
    (entry) =>
      entry.direction === "capture" &&
      (showSystemStreams.value || !entry.is_system) &&
      !entry.is_live,
  ),
);

const hasInputs = computed(() => inputDevices.value.length > 0);
const hasAnyContent = computed(
  () =>
    captureStreams.value.length > 0 ||
    inputDevices.value.length > 0 ||
    recentCaptureIdentities.value.length > 0,
);

const isEmpty = computed(
  () => !loading.value && !error.value && !hasAnyContent.value,
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

function recentLabel(entry: RecentStreamIdentity): string {
  if (entry.media_name && entry.media_name !== entry.app_name) {
    return `${entry.app_name} (${entry.media_name})`;
  }
  return entry.app_name;
}

async function saveRename(device: Device, alias: string) {
  try {
    await invoke("set_device_alias", { systemName: device.system_name, alias });
    handleApplyResult({ success: true }, "Device renamed");
  } catch (err) {
    handleApplyResult(
      { success: false, message: err instanceof Error ? err.message : String(err) },
      "",
    );
  }
}

async function removeVirtual(device: Device) {
  const confirmed = await confirm(`Delete virtual device "${device.label}"?`, {
    title: "Delete virtual device",
    confirmLabel: "Delete",
    cancelLabel: "Cancel",
  });
  if (!confirmed) return;

  try {
    await invoke("remove_virtual_device", { systemName: device.system_name });
    handleApplyResult({ success: true }, "Virtual device removed");
  } catch (err) {
    handleApplyResult(
      { success: false, message: err instanceof Error ? err.message : String(err) },
      "",
    );
  }
}
</script>

<template>
  <div class="sources-view">
    <header class="sources-header view-header">
      <div>
        <p class="eyebrow">Capture and inputs</p>
        <h1>Sources</h1>
      </div>
      <div class="sources-actions view-actions">
        <div class="header-toggle">
          <span class="toggle-row-label">Show system streams</span>
          <ToggleSwitch
            :model-value="showSystemStreams"
            :show-state-labels="false"
            @update:model-value="onToggleSystemStreams"
          />
        </div>
        <button type="button" @click="refresh">Refresh</button>
      </div>
    </header>

    <p v-if="isMockData" class="notice-banner mock">
      {{ graph.notice ?? "Showing sample data (PIPE_DECK_USE_MOCK=1)." }}
    </p>
    <p v-else-if="graph.notice" class="notice-banner warn">
      {{ graph.notice }}
    </p>

    <div
      v-if="!loading && !error && playbackStreams.length > 0 && captureStreams.length === 0"
      class="sources-playback-hint notice-banner"
    >
      <p>
        <strong>{{ playbackStreams.length }} playback app{{ playbackStreams.length === 1 ? "" : "s" }}</strong>
        active (e.g. music players). Playback is routed on
        <button v-if="navigate" type="button" class="inline-link" @click="navigate('dashboard')">
          Dashboard
        </button>
        <template v-else>Dashboard</template>
        or Routing. Capture streams appear here when an app is recording from your microphone.
      </p>
    </div>

    <p v-if="loading" class="status">Loading runtime graph…</p>
    <p v-else-if="error" class="status error">{{ error }}</p>
    <p v-else-if="isEmpty" class="status">
      No input devices detected. Connect a microphone or create a virtual input.
    </p>

    <template v-else>
      <section class="sources-section">
        <h2>Capture streams</h2>
        <p v-if="captureStreams.length === 0 && hasInputs" class="empty">
          No apps are recording right now. Inputs are available below — open an app and select a
          microphone to route it here.
        </p>
        <p v-else-if="captureStreams.length === 0" class="empty">
          No apps are recording from a microphone right now. Music and media players are playback
          streams — use Dashboard or Routing to route them. Try enabling “Show system streams” if
          you expect a mic capture app.
        </p>
        <div v-else class="sources-card-list">
          <StreamTargetPicker
            v-for="stream in captureStreams"
            :key="stream.id"
            :stream="stream"
            :devices="displayGraph.devices"
          />
        </div>

        <div v-if="captureStreams.length === 0 && recentCaptureIdentities.length > 0" class="sources-recent">
          <h3>Recently seen</h3>
          <p class="section-help">
            These apps used a microphone recently. Start capture in the app to route live.
          </p>
          <ul class="sources-recent-list">
            <li v-for="(entry, index) in recentCaptureIdentities" :key="`${entry.app_name}-${index}`">
              <strong>{{ recentLabel(entry) }}</strong>
              <span v-if="entry.executable" class="node-sub">{{ entry.executable }}</span>
            </li>
          </ul>
        </div>
      </section>

      <section class="sources-section">
        <h2>Input devices</h2>
        <p v-if="inputDevices.length === 0" class="empty">No input devices detected.</p>
        <div v-else class="sources-device-grid">
          <article
            v-for="device in inputDevices"
            :key="device.id"
            class="sources-device-card"
          >
            <span class="node-icon input">🎤</span>
            <div class="sources-device-body">
              <NodeCardHeader
                :label="device.label"
                editable
                :deletable="
                  device.kind === 'virtual' && device.system_name.startsWith('pipe-deck-')
                "
                @save="(name) => saveRename(device, name)"
                @delete="removeVirtual(device)"
              />
              <span class="node-sub">{{ deviceSubtitle(device) }}</span>
            </div>
          </article>
        </div>
      </section>
    </template>
  </div>
</template>
