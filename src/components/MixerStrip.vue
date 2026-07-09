<script setup lang="ts">
import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import NodeCardHeader from "./NodeCardHeader.vue";
import { useApplyResult } from "../stores/notices";
import { useConfirm } from "../stores/confirm";
import type { Device } from "../types/graph";

const props = defineProps<{
  devices: Device[];
}>();

const { handleApplyResult } = useApplyResult();
const { confirm } = useConfirm();
const pendingVolumes = ref<Record<string, number>>({});
let debounceTimers: Record<string, number> = {};

interface MixerChannel {
  id: string;
  label: string;
  systemName: string;
  direction: Device["direction"];
  kind: Device["kind"];
  level: number;
  muted: boolean;
}

function toChannel(device: Device): MixerChannel | null {
  if (device.volume_percent === undefined) {
    return null;
  }

  return {
    id: device.id,
    label: device.label,
    systemName: device.system_name,
    direction: device.direction,
    kind: device.kind,
    level: pendingVolumes.value[device.id] ?? device.volume_percent,
    muted: device.muted ?? false,
  };
}

const outputChannels = computed(() =>
  props.devices
    .filter((device) => device.direction === "output")
    .map(toChannel)
    .filter((channel): channel is MixerChannel => channel !== null),
);

const inputChannels = computed(() =>
  props.devices
    .filter((device) => device.direction === "input")
    .map(toChannel)
    .filter((channel): channel is MixerChannel => channel !== null),
);

const hasChannels = computed(
  () => outputChannels.value.length > 0 || inputChannels.value.length > 0,
);

function scheduleVolume(deviceId: string, percent: number) {
  pendingVolumes.value[deviceId] = percent;
  window.clearTimeout(debounceTimers[deviceId]);
  debounceTimers[deviceId] = window.setTimeout(async () => {
    try {
      await invoke("set_device_volume", { deviceId, percent });
    } catch (error) {
      handleApplyResult(
        { success: false, message: error instanceof Error ? error.message : String(error) },
        "",
      );
    }
  }, 120);
}

async function toggleMute(channel: MixerChannel) {
  try {
    await invoke("set_device_mute", { deviceId: channel.id, muted: !channel.muted });
    handleApplyResult({ success: true }, channel.muted ? "Unmuted" : "Muted");
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

async function saveRename(channel: MixerChannel, alias: string) {
  try {
    await invoke("set_device_alias", { systemName: channel.systemName, alias });
    handleApplyResult({ success: true }, "Device renamed");
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

async function removeVirtual(channel: MixerChannel) {
  const confirmed = await confirm(`Delete virtual device "${channel.label}"?`, {
    title: "Delete virtual device",
    confirmLabel: "Delete",
    cancelLabel: "Cancel",
  });
  if (!confirmed) {
    return;
  }

  try {
    await invoke("remove_virtual_device", { systemName: channel.systemName });
    handleApplyResult({ success: true }, "Virtual device removed");
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}
</script>

<template>
  <footer class="mixer-strip">
    <template v-if="hasChannels">
      <section v-if="outputChannels.length" class="mixer-group">
        <h3>Outputs</h3>
        <div class="channel-grid">
          <article
            v-for="channel in outputChannels"
            :key="channel.id"
            class="channel"
          >
            <div class="channel-top">
              <NodeCardHeader
                :label="channel.label"
                editable
                :deletable="channel.kind === 'virtual'"
                @save="(name) => saveRename(channel, name)"
                @delete="removeVirtual(channel)"
              />
              <span class="channel-badge">Output</span>
            </div>
            <div class="slider-row">
              <input
                type="range"
                min="0"
                max="100"
                :value="channel.level"
                :aria-label="`${channel.label} volume`"
                @input="scheduleVolume(channel.id, Number(($event.target as HTMLInputElement).value))"
              />
              <span class="level">{{ channel.level }}%</span>
              <button
                type="button"
                class="mute"
                :class="{ active: channel.muted }"
                :aria-label="channel.muted ? 'Muted' : 'Unmuted'"
                @click="toggleMute(channel)"
              >
                {{ channel.muted ? "🔇" : "🔊" }}
              </button>
            </div>
          </article>
        </div>
      </section>

      <section v-if="inputChannels.length" class="mixer-group">
        <h3>Inputs</h3>
        <div class="channel-grid">
          <article
            v-for="channel in inputChannels"
            :key="channel.id"
            class="channel"
          >
            <div class="channel-top">
              <NodeCardHeader
                :label="channel.label"
                editable
                :deletable="channel.kind === 'virtual'"
                @save="(name) => saveRename(channel, name)"
                @delete="removeVirtual(channel)"
              />
              <span class="channel-badge input">Input</span>
            </div>
            <div class="slider-row">
              <input
                type="range"
                min="0"
                max="100"
                :value="channel.level"
                :aria-label="`${channel.label} volume`"
                @input="scheduleVolume(channel.id, Number(($event.target as HTMLInputElement).value))"
              />
              <span class="level">{{ channel.level }}%</span>
              <button
                type="button"
                class="mute"
                :class="{ active: channel.muted }"
                :aria-label="channel.muted ? 'Muted' : 'Unmuted'"
                @click="toggleMute(channel)"
              >
                {{ channel.muted ? "🔇" : "🔊" }}
              </button>
            </div>
          </article>
        </div>
      </section>
    </template>

    <p v-else class="empty">No mixer channels detected.</p>
  </footer>
</template>
