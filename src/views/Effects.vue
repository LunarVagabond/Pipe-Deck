<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import ToggleSwitch from "../components/ToggleSwitch.vue";
import { useApplyResult } from "../stores/notices";
import { useRuntimeGraph } from "../stores/runtimeGraph";
import {
  emptyDynamicsStage,
  type Device,
  type DynamicsStage,
  type EffectChainConfig,
  type FxCapabilities,
} from "../types/graph";

const { graph, loading, error, refresh } = useRuntimeGraph();
const { handleApplyResult } = useApplyResult();

const chains = ref<Record<string, EffectChainConfig>>({});
const selectedDeviceId = ref<string | null>(null);
const draft = ref<EffectChainConfig>(emptyChain());
const chainsLoading = ref(true);
const saveState = ref<"idle" | "saving" | "saved" | "error">("idle");
const capabilities = ref<FxCapabilities>({ builtin_eq: false, builtin_gain: false, builtin_limiter: false });
let debounceTimer: number | undefined;
let savedIndicatorTimer: number | undefined;

const eqBands = [
  { key: "eq_sub" as const, label: "Sub", hint: "60 Hz" },
  { key: "eq_bass" as const, label: "Bass", hint: "150 Hz" },
  { key: "eq_mid" as const, label: "Mid", hint: "1 kHz" },
  { key: "eq_treble" as const, label: "Treble", hint: "4 kHz" },
  { key: "eq_air" as const, label: "Air", hint: "10 kHz" },
];

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

function emptyChain(): EffectChainConfig {
  return {
    eq_sub: 0,
    eq_bass: 0,
    eq_mid: 0,
    eq_treble: 0,
    eq_air: 0,
    output_gain: 0,
    compressor: emptyDynamicsStage(),
    limiter: emptyDynamicsStage(),
    noise_gate: emptyDynamicsStage(),
  };
}

/** Accepts a legacy bare `boolean` for `compressor` (pre-dynamics-suite
 * configs) in addition to the current `DynamicsStage` object, mirroring the
 * same migration the Rust side does for on-disk configs. */
function normalizeDynamicsStage(value: DynamicsStage | boolean | undefined): DynamicsStage {
  if (typeof value === "boolean") {
    return { ...emptyDynamicsStage(), enabled: value };
  }
  return value ?? emptyDynamicsStage();
}

function normalizeChain(chain: EffectChainConfig): EffectChainConfig {
  return {
    eq_sub: chain.eq_sub ?? 0,
    eq_bass: chain.eq_bass ?? chain.eq_low ?? 0,
    eq_mid: chain.eq_mid ?? 0,
    eq_treble: chain.eq_treble ?? 0,
    eq_air: chain.eq_air ?? chain.eq_high ?? 0,
    output_gain: chain.output_gain ?? 0,
    compressor: normalizeDynamicsStage(chain.compressor),
    limiter: normalizeDynamicsStage(chain.limiter),
    noise_gate: normalizeDynamicsStage(chain.noise_gate),
  };
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

const showBlockingLoader = computed(
  () =>
    chainsLoading.value ||
    (loading.value && eligibleDevices.value.length === 0 && !error.value),
);

const isEmpty = computed(
  () => !showBlockingLoader.value && !error.value && eligibleDevices.value.length === 0,
);

const isChainActive = computed(() => {
  const chain = draft.value;
  return (
    chain.compressor.enabled ||
    chain.limiter.enabled ||
    chain.noise_gate.enabled ||
    chain.eq_sub !== 0 ||
    chain.eq_bass !== 0 ||
    chain.eq_mid !== 0 ||
    chain.eq_treble !== 0 ||
    chain.eq_air !== 0 ||
    chain.output_gain !== 0
  );
});

function toggleDynamicsStage(key: "compressor" | "limiter" | "noise_gate", enabled: boolean) {
  draft.value = {
    ...draft.value,
    [key]: { ...draft.value[key], enabled },
  };
  scheduleApply();
}

async function loadChains() {
  chainsLoading.value = true;
  try {
    const loaded = await invoke<Record<string, EffectChainConfig>>("get_effect_chains");
    chains.value = Object.fromEntries(
      Object.entries(loaded).map(([id, chain]) => [id, normalizeChain(chain)]),
    );
  } catch {
    chains.value = {};
  } finally {
    chainsLoading.value = false;
  }
}

function loadDraftForDevice(deviceId: string) {
  draft.value = normalizeChain(chains.value[deviceId] ?? emptyChain());
}

function selectDevice(deviceId: string) {
  selectedDeviceId.value = deviceId;
  loadDraftForDevice(deviceId);
}

async function applyDraft() {
  if (!selectedDeviceId.value) {
    return;
  }

  const deviceId = selectedDeviceId.value;
  const config = normalizeChain({ ...draft.value });

  saveState.value = "saving";
  try {
    await invoke("set_device_effects", {
      deviceId,
      config,
    });
    if (!isChainActive.value) {
      const { [deviceId]: _, ...rest } = chains.value;
      chains.value = rest;
    } else {
      chains.value = { ...chains.value, [deviceId]: config };
    }
    saveState.value = "saved";
    window.clearTimeout(savedIndicatorTimer);
    savedIndicatorTimer = window.setTimeout(() => {
      if (saveState.value === "saved") {
        saveState.value = "idle";
      }
    }, 1500);
  } catch (err) {
    saveState.value = "error";
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
  }, 200);
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

async function loadCapabilities() {
  try {
    capabilities.value = await invoke<FxCapabilities>("get_effect_capabilities");
  } catch {
    capabilities.value = { builtin_eq: false, builtin_gain: false, builtin_limiter: false };
  }
}

onMounted(() => {
  void loadCapabilities();
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
      Five-band EQ and dynamics settings are saved for profiles. Live processing is temporarily
      disabled while we rework the PipeWire integration — adjusting sliders will not touch your
      system audio session.
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
          <div class="effects-panel-header">
            <div>
              <h2>{{ selectedDevice.label }}</h2>
              <p class="effects-panel-subtitle">{{ selectedDevice.system_name }}</p>
            </div>
            <p
              v-if="saveState !== 'idle'"
              class="effects-save-state"
              :class="saveState"
            >
              {{ saveState === "saving" ? "Saving…" : saveState === "saved" ? "Saved" : "Save failed" }}
            </p>
          </div>

          <p class="notice-banner info effects-live-disabled">
            Settings save to your profile, but sliders do not change audio yet. Live EQ is
            paused while we rebuild a PipeWire-safe effects path.
          </p>

          <div class="effects-section">
            <h3>Equalizer</h3>
            <div
              v-for="band in eqBands"
              :key="band.key"
              class="effects-control"
            >
              <label>
                <span class="effects-band-label">
                  {{ band.label }}
                  <em>{{ band.hint }}</em>
                </span>
                <input
                  v-model.number="draft[band.key]"
                  type="range"
                  min="-12"
                  max="12"
                  step="1"
                  @input="scheduleApply"
                />
                <span class="value">{{ draft[band.key] }}</span>
              </label>
            </div>
          </div>

          <div class="effects-section">
            <h3>Dynamics</h3>
            <div class="effects-control">
              <label>
                <span class="effects-band-label">
                  Output
                  <em>trim</em>
                </span>
                <input
                  v-model.number="draft.output_gain"
                  type="range"
                  min="-12"
                  max="12"
                  step="1"
                  @input="scheduleApply"
                />
                <span class="value">{{ draft.output_gain }}</span>
              </label>
            </div>

            <div
              v-for="stage in dynamicsStages"
              :key="stage.key"
              class="effects-control effects-toggle-row"
              :class="{ disabled: !stage.available }"
              :title="stage.available ? undefined : stage.unavailableReason"
            >
              <span>{{ stage.label }}</span>
              <ToggleSwitch
                :model-value="draft[stage.key].enabled"
                :disabled="!stage.available"
                :show-state-labels="false"
                @update:model-value="(next) => toggleDynamicsStage(stage.key, next)"
              />
            </div>
          </div>

          <button type="button" class="effects-reset" @click="draft = emptyChain(); scheduleApply();">
            Reset chain
          </button>
        </section>
      </div>
    </template>
  </div>
</template>
