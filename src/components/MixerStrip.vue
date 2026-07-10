<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import NodeCardHeader from "./NodeCardHeader.vue";
import { useApplyResult } from "../stores/notices";
import { useConfirm } from "../stores/confirm";
import type { Device, Stream } from "../types/graph";

const props = withDefaults(
  defineProps<{
    devices: Device[];
    streams?: Stream[];
  }>(),
  {
    streams: () => [],
  },
);

const { handleApplyResult } = useApplyResult();
const { confirm } = useConfirm();
const pendingVolumes = ref<Record<string, number>>({});
const editingVolumeId = ref<string | null>(null);
const volumeDraft = ref("");
const volumeInputRef = ref<HTMLInputElement | null>(null);
let debounceTimers: Record<string, number> = {};

interface MixerChannel {
  id: string;
  label: string;
  systemName?: string;
  direction: Device["direction"] | Stream["direction"];
  kind?: Device["kind"];
  channelType: "device" | "stream";
  level: number;
  muted: boolean;
}

function toDeviceChannel(device: Device): MixerChannel | null {
  if (device.volume_percent === undefined) {
    return null;
  }

  return {
    id: device.id,
    label: device.label,
    systemName: device.system_name,
    direction: device.direction,
    kind: device.kind,
    channelType: "device",
    level: pendingVolumes.value[device.id] ?? device.volume_percent,
    muted: device.muted ?? false,
  };
}

function toStreamChannel(stream: Stream): MixerChannel | null {
  if (stream.volume_percent === undefined || stream.is_system) {
    return null;
  }

  return {
    id: stream.id,
    label: stream.media_name
      ? `${stream.app_name} (${stream.media_name})`
      : stream.app_name,
    direction: stream.direction,
    channelType: "stream",
    level: pendingVolumes.value[stream.id] ?? stream.volume_percent,
    muted: stream.muted ?? false,
  };
}

const outputChannels = computed(() =>
  props.devices
    .filter((device) => device.direction === "output")
    .map(toDeviceChannel)
    .filter((channel): channel is MixerChannel => channel !== null),
);

const inputChannels = computed(() =>
  props.devices
    .filter((device) => device.direction === "input")
    .map(toDeviceChannel)
    .filter((channel): channel is MixerChannel => channel !== null),
);

const applicationChannels = computed(() =>
  props.streams
    .map(toStreamChannel)
    .filter((channel): channel is MixerChannel => channel !== null),
);

const hasChannels = computed(
  () =>
    outputChannels.value.length > 0 ||
    inputChannels.value.length > 0 ||
    applicationChannels.value.length > 0,
);

const mixerSections = computed(() =>
  [
    { title: "Applications", channels: applicationChannels.value },
    { title: "Outputs", channels: outputChannels.value },
    { title: "Inputs", channels: inputChannels.value },
  ].filter((section) => section.channels.length > 0),
);

function clampVolume(value: number) {
  return Math.min(100, Math.max(0, Math.round(value)));
}

async function applyVolume(channel: MixerChannel, percent: number) {
  const next = clampVolume(percent);
  pendingVolumes.value[channel.id] = next;
  const command =
    channel.channelType === "stream" ? "set_stream_volume" : "set_device_volume";
  const payload =
    channel.channelType === "stream"
      ? { streamId: channel.id, percent: next }
      : { deviceId: channel.id, percent: next };

  try {
    await invoke(command, payload);
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

function scheduleVolume(channel: MixerChannel, percent: number) {
  pendingVolumes.value[channel.id] = clampVolume(percent);
  window.clearTimeout(debounceTimers[channel.id]);
  debounceTimers[channel.id] = window.setTimeout(() => {
    void applyVolume(channel, pendingVolumes.value[channel.id]);
  }, 120);
}

async function startVolumeEdit(channel: MixerChannel) {
  editingVolumeId.value = channel.id;
  volumeDraft.value = String(channel.level);
  await nextTick();
  volumeInputRef.value?.focus();
  volumeInputRef.value?.select();
}

function cancelVolumeEdit(channel: MixerChannel) {
  if (editingVolumeId.value === channel.id) {
    editingVolumeId.value = null;
  }
}

async function commitVolumeEdit(channel: MixerChannel) {
  if (editingVolumeId.value !== channel.id) {
    return;
  }

  const parsed = Number(volumeDraft.value);
  const percent = Number.isFinite(parsed) ? clampVolume(parsed) : channel.level;
  editingVolumeId.value = null;
  await applyVolume(channel, percent);
}

async function toggleMute(channel: MixerChannel) {
  const command =
    channel.channelType === "stream" ? "set_stream_mute" : "set_device_mute";
  const payload =
    channel.channelType === "stream"
      ? { streamId: channel.id, muted: !channel.muted }
      : { deviceId: channel.id, muted: !channel.muted };

  try {
    await invoke(command, payload);
    handleApplyResult({ success: true }, channel.muted ? "Unmuted" : "Muted");
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

async function saveRename(channel: MixerChannel, alias: string) {
  if (!channel.systemName) {
    return;
  }
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
  if (!channel.systemName) {
    return;
  }
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
      <section v-for="section in mixerSections" :key="section.title" class="mixer-group">
        <h3>{{ section.title }}</h3>
        <div class="channel-grid">
          <article
            v-for="channel in section.channels"
            :key="channel.id"
            class="channel"
          >
            <div class="channel-slider">
              <div class="level-wrap">
                <input
                  v-if="editingVolumeId === channel.id"
                  ref="volumeInputRef"
                  class="level-input"
                  type="number"
                  min="0"
                  max="100"
                  inputmode="numeric"
                  :aria-label="`${channel.label} volume percent`"
                  v-model="volumeDraft"
                  @blur="commitVolumeEdit(channel)"
                  @keydown.enter.prevent="commitVolumeEdit(channel)"
                  @keydown.esc.prevent="cancelVolumeEdit(channel)"
                />
                <button
                  v-else
                  type="button"
                  class="level"
                  :aria-label="`Set ${channel.label} volume`"
                  @click="startVolumeEdit(channel)"
                >
                  {{ channel.level }}%
                </button>
              </div>
              <div class="volume-vertical-wrap">
                <div
                  class="meter-fill"
                  :style="{ height: `${channel.level}%` }"
                  aria-hidden="true"
                />
                <input
                  type="range"
                  class="volume-vertical"
                  min="0"
                  max="100"
                  :value="channel.level"
                  :aria-label="`${channel.label} volume`"
                  @input="scheduleVolume(channel, Number(($event.target as HTMLInputElement).value))"
                />
              </div>
            </div>
            <div class="channel-footer">
              <NodeCardHeader
                v-if="channel.channelType === 'device'"
                layout="stacked"
                show-label-tooltip
                :label="channel.label"
                editable
                :deletable="channel.kind === 'virtual'"
                @save="(name) => saveRename(channel, name)"
                @delete="removeVirtual(channel)"
              >
                <template #toolbar-extra>
                  <button
                    type="button"
                    class="mute"
                    :class="{ active: channel.muted }"
                    :aria-label="channel.muted ? 'Muted' : 'Unmuted'"
                    @click="toggleMute(channel)"
                  >
                    {{ channel.muted ? "🔇" : "🔊" }}
                  </button>
                </template>
              </NodeCardHeader>
              <div v-else class="stream-channel-footer">
                <p class="stream-channel-label" :title="channel.label">{{ channel.label }}</p>
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
            </div>
          </article>
        </div>
      </section>
    </template>

    <p v-else class="empty">No mixer channels detected.</p>
  </footer>
</template>
