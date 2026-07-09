<script setup lang="ts">
import { computed } from "vue";
import type { Device } from "../types/graph";

const props = defineProps<{
  devices: Device[];
}>();

interface MixerChannel {
  id: string;
  label: string;
  direction: Device["direction"];
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
    direction: device.direction,
    level: device.volume_percent,
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
            <div class="channel-header">
              <span class="channel-label">{{ channel.label }}</span>
              <span class="channel-badge">Output</span>
            </div>
            <div class="slider-row">
              <input
                type="range"
                min="0"
                max="100"
                :value="channel.level"
                disabled
                :aria-label="`${channel.label} volume`"
              />
              <span class="level">{{ channel.level }}%</span>
              <button
                type="button"
                class="mute"
                :class="{ active: channel.muted }"
                disabled
                :aria-label="channel.muted ? 'Muted' : 'Unmuted'"
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
            <div class="channel-header">
              <span class="channel-label">{{ channel.label }}</span>
              <span class="channel-badge input">Input</span>
            </div>
            <div class="slider-row">
              <input
                type="range"
                min="0"
                max="100"
                :value="channel.level"
                disabled
                :aria-label="`${channel.label} volume`"
              />
              <span class="level">{{ channel.level }}%</span>
              <button
                type="button"
                class="mute"
                :class="{ active: channel.muted }"
                disabled
                :aria-label="channel.muted ? 'Muted' : 'Unmuted'"
              >
                {{ channel.muted ? "🔇" : "🔊" }}
              </button>
            </div>
          </article>
        </div>
      </section>
    </template>

    <p v-else class="empty">No mixer channels detected.</p>

    <p class="note">Read-only levels from PipeWire. Interactive mixer controls coming soon.</p>
  </footer>
</template>
