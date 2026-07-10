<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import ToggleSwitch from "../components/ToggleSwitch.vue";
import { useApplyResult } from "../stores/notices";
import { useRuntimeGraph } from "../stores/runtimeGraph";
import type { Device, EffectChainConfig } from "../types/graph";

const { graph, loading, error, refresh } = useRuntimeGraph();
const { handleApplyResult } = useApplyResult();

const chains = ref<Record<string, EffectChainConfig>>({});
const selectedDeviceId = ref<string | null>(null);
const draft = ref<EffectChainConfig>(emptyChain());
const chainsLoading = ref(true);
const applyWarning = ref<string | null>(null);
let debounceTimer: number | undefined;

function emptyChain(): EffectChainConfig {
  return { eq_low: 0, eq_mid: 0, eq_high: 0, compressor: false };
}

function isEffectsDevice(device: Device): boolean {
  return (
    device.kind === "virtual" &&
    device.system_name.startsWith("pipe-deck-") &&
    !device.system_name.startsWith("pipe-deck-feed-") &&
    !device.system_name.startsWith("pipe-deck-split-")
  );
}

const eligibleDevices = computed(() =>
  graph.value.devices.filter(isEffectsDevice),
);

const selectedDevice = computed(() =>
  eligibleDevices.value.find((device) => device.id === selectedDeviceId.value) ?? null,
);

const isMockData = computed(() => graph.value.data_source === "mock");

const isEmpty = computed(
  () => !loading.value && !error.value && eligibleDevices.value.length === 0,
);

async function loadChains() {
  chainsLoading.value = true;
  try {
    chains.value = await invoke<Record<string, EffectChainConfig>>("get_effect_chains");
  } catch {
    chains.value = {};
  } finally {
    chainsLoading.value = false;
  }
}

function loadDraftForDevice(deviceId: string) {
  draft.value = { ...(chains.value[deviceId] ?? emptyChain()) };
}

function selectDevice(deviceId: string) {
  selectedDeviceId.value = deviceId;
  loadDraftForDevice(deviceId);
  applyWarning.value = null;
}

async function applyDraft() {
  if (!selectedDeviceId.value) {
    return;
  }

  const deviceId = selectedDeviceId.value;
  const config = { ...draft.value };

  try {
    const result = await invoke<{ success: boolean; message?: string }>("set_device_effects", {
      deviceId,
      config,
    });
    if (config.eq_low === 0 && config.eq_mid === 0 && config.eq_high === 0 && !config.compressor) {
      const { [deviceId]: _, ...rest } = chains.value;
      chains.value = rest;
    } else {
      chains.value = { ...chains.value, [deviceId]: config };
    }
    applyWarning.value = result.message ?? null;
    handleApplyResult(
      result,
      result.message ? "Effects saved (with warning)" : "Effects applied",
    );
  } catch (err) {
    handleApplyResult(
      { success: false, message: err instanceof Error ? err.message : String(err) },
      "",
    );
  }
}

function scheduleApply() {
  window.clearTimeout(debounceTimer);
  debounceTimer = window.setTimeout(() => {
    void applyDraft();
  }, 250);
}

watch(
  eligibleDevices,
  (devices) => {
    if (devices.length === 0) {
      selectedDeviceId.value = null;
      return;
    }
    if (!selectedDeviceId.value || !devices.some((device) => device.id === selectedDeviceId.value)) {
      selectDevice(devices[0].id);
    }
  },
  { immediate: true },
);

onMounted(() => {
  void loadChains().then(() => {
    if (selectedDeviceId.value) {
      loadDraftForDevice(selectedDeviceId.value);
    }
  });
});
</script>

<template>
  <div class="effects-view">
    <header class="effects-header">
      <div>
        <p class="eyebrow">Virtual device processing</p>
        <h1>Effects</h1>
      </div>
      <div class="effects-actions">
        <button type="button" @click="refresh">Refresh</button>
      </div>
    </header>

    <p class="effects-help">
      Apply a 3-band EQ and compressor to Pipe Deck virtual devices. Changes persist in
      <code>config.yaml</code> and are captured when you save a profile.
    </p>

    <p v-if="isMockData" class="notice-banner mock">
      {{ graph.notice ?? "Showing sample data (PIPE_DECK_USE_MOCK=1)." }}
    </p>
    <p v-else-if="graph.notice" class="notice-banner warn">
      {{ graph.notice }}
    </p>

    <p v-if="loading || chainsLoading" class="status">Loading devices…</p>
    <p v-else-if="error" class="status error">{{ error }}</p>
    <p v-else-if="isEmpty" class="status">
      No Pipe Deck virtual devices available. Create a virtual output from + New first.
    </p>

    <template v-else>
      <div class="effects-layout">
        <section class="effects-device-list">
          <h2>Devices</h2>
          <button
            v-for="device in eligibleDevices"
            :key="device.id"
            type="button"
            class="effects-device-btn"
            :class="{ active: device.id === selectedDeviceId }"
            @click="selectDevice(device.id)"
          >
            <strong>{{ device.label }}</strong>
            <span>{{ device.system_name }}</span>
          </button>
        </section>

        <section v-if="selectedDevice" class="effects-panel">
          <h2>{{ selectedDevice.label }}</h2>
          <p class="effects-panel-subtitle">{{ selectedDevice.system_name }}</p>

          <p v-if="applyWarning" class="notice-banner warn">{{ applyWarning }}</p>

          <div class="effects-control">
            <label>
              <span>Low EQ</span>
              <input
                v-model.number="draft.eq_low"
                type="range"
                min="-12"
                max="12"
                step="1"
                @input="scheduleApply"
              />
              <span class="value">{{ draft.eq_low }}</span>
            </label>
          </div>

          <div class="effects-control">
            <label>
              <span>Mid EQ</span>
              <input
                v-model.number="draft.eq_mid"
                type="range"
                min="-12"
                max="12"
                step="1"
                @input="scheduleApply"
              />
              <span class="value">{{ draft.eq_mid }}</span>
            </label>
          </div>

          <div class="effects-control">
            <label>
              <span>High EQ</span>
              <input
                v-model.number="draft.eq_high"
                type="range"
                min="-12"
                max="12"
                step="1"
                @input="scheduleApply"
              />
              <span class="value">{{ draft.eq_high }}</span>
            </label>
          </div>

          <div class="effects-control effects-toggle-row">
            <span>Compressor</span>
            <ToggleSwitch
              :model-value="draft.compressor"
              :show-state-labels="false"
              @update:model-value="(next) => { draft.compressor = next; scheduleApply(); }"
            />
          </div>

          <button type="button" class="effects-reset" @click="draft = emptyChain(); scheduleApply();">
            Reset chain
          </button>
        </section>
      </div>
    </template>
  </div>
</template>
