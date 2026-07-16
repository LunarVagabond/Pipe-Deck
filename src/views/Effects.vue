<script setup lang="ts">
import { computed, watch } from "vue";
import ToggleSwitch from "../components/ToggleSwitch.vue";
import EffectStageList from "../components/EffectStageList.vue";
import { useRuntimeGraph } from "../stores/runtimeGraph";
import { useEffectChain } from "../composables/useEffectChain";
import { ref } from "vue";
import type { Device } from "../types/graph";

const { graph, loading, error, refresh } = useRuntimeGraph();
const { chainFor, capabilities, loading: chainsLoading, setBypassed, setDynamicsStageEnabled } = useEffectChain();

const selectedDeviceId = ref<string | null>(null);

const dynamicsStages = computed(() => [
  {
    key: "compressor" as const,
    label: "Compressor",
    available: false,
    unavailableReason: "No supported backing plugin on this system yet",
  },
  {
    key: "limiter" as const,
    label: "Limiter",
    available: capabilities.value.builtin_limiter,
    unavailableReason: "PipeWire has no builtin limiter plugin on this system",
  },
  {
    key: "noise_gate" as const,
    label: "Noise gate",
    available: Boolean(capabilities.value.ladspa_noise_gate),
    unavailableReason: "Requires a LADSPA noise-suppression plugin (e.g. librnnoise) not found on this system",
  },
]);

function isEffectsDevice(device: Device): boolean {
  return (
    device.kind === "virtual" &&
    device.direction !== "duplex" &&
    device.system_name.startsWith("pipe-deck-") &&
    !device.system_name.startsWith("pipe-deck-feed-") &&
    !device.system_name.startsWith("pipe-deck-split-")
  );
}

const eligibleDevices = computed(() => graph.value.devices.filter(isEffectsDevice));

const selectedDevice = computed(() =>
  eligibleDevices.value.find((device) => device.id === selectedDeviceId.value) ?? null,
);

const isMockData = computed(() => graph.value.data_source === "mock");

const showBlockingLoader = computed(
  () => chainsLoading.value || (loading.value && eligibleDevices.value.length === 0 && !error.value),
);

const isEmpty = computed(
  () => !showBlockingLoader.value && !error.value && eligibleDevices.value.length === 0,
);

function selectDevice(deviceId: string) {
  selectedDeviceId.value = deviceId;
}

function toggleDynamicsStage(key: "compressor" | "limiter" | "noise_gate", enabled: boolean) {
  if (!selectedDevice.value) return;
  void setDynamicsStageEnabled(selectedDevice.value.id, key, enabled);
}

watch(
  eligibleDevices,
  (devices) => {
    if (devices.length === 0) {
      selectedDeviceId.value = null;
      return;
    }
    if (!selectedDeviceId.value || !devices.some((device) => device.id === selectedDeviceId.value)) {
      selectedDeviceId.value = devices[0].id;
    }
  },
  { immediate: true },
);
</script>

<template>
  <div class="effects-view">
    <header class="effects-header view-header">
      <div>
        <p class="eyebrow">Virtual device processing</p>
      </div>
      <div class="effects-actions">
        <button type="button" @click="refresh">Refresh</button>
      </div>
    </header>

    <p class="effects-help">
      Right-click a device on the Routing graph (or a channel in Mixer) to add an effect directly —
      it applies immediately, no separate enable step. This page is the same effect chains as a flat
      list, useful when you'd rather not hunt across the graph.
    </p>

    <p v-if="isMockData" class="notice-banner mock">
      {{ graph.notice ?? "Showing sample data (PIPE_DECK_USE_MOCK=1)." }}
    </p>
    <p v-else-if="graph.notice" class="notice-banner warn">
      {{ graph.notice }}
    </p>

    <p v-if="showBlockingLoader" class="status">Loading devices…</p>
    <p v-else-if="error" class="status error">{{ error }}</p>
    <p v-else-if="isEmpty" class="status">
      No Pipe Deck virtual devices available. Create a virtual output or virtual input from + New first.
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
          <div class="effects-panel-header">
            <div>
              <h2>{{ selectedDevice.label }}</h2>
              <p class="effects-panel-subtitle">{{ selectedDevice.system_name }}</p>
            </div>
            <div class="effects-panel-header-actions">
              <label
                class="effects-bypass-toggle"
                title="Keeps your effect chain configured but stops it from affecting audio — nothing is removed."
              >
                <ToggleSwitch
                  :model-value="chainFor(selectedDevice.id).bypassed"
                  :disabled="chainFor(selectedDevice.id).stages.length === 0"
                  :show-state-labels="false"
                  @update:model-value="(next) => setBypassed(selectedDevice!.id, next)"
                />
                <span>Bypass</span>
              </label>
            </div>
          </div>

          <p v-if="!capabilities.builtin_eq" class="notice-banner warn effects-live-disabled">
            Live EQ isn't available on this system (PipeWire's filter-chain module wasn't found).
          </p>

          <div class="effects-section">
            <h3>Effects</h3>
            <EffectStageList :device-id="selectedDevice.id" />
          </div>

          <div class="effects-section">
            <h3>Dynamics</h3>
            <div
              v-for="stage in dynamicsStages"
              :key="stage.key"
              class="effects-control effects-toggle-row"
              :class="{ disabled: !stage.available }"
              :title="stage.available ? undefined : stage.unavailableReason"
            >
              <span>{{ stage.label }}</span>
              <ToggleSwitch
                :model-value="chainFor(selectedDevice.id)[stage.key].enabled"
                :disabled="!stage.available"
                :show-state-labels="false"
                @update:model-value="(next) => toggleDynamicsStage(stage.key, next)"
              />
            </div>
          </div>
        </section>
      </div>
    </template>
  </div>
</template>
