<script setup lang="ts">
import { computed } from "vue";
import { useMixerControls } from "../composables/useMixerControls";
import EffectStageList from "./EffectStageList.vue";

const props = defineProps<{
  channelType: "device" | "stream";
  entityId: string;
  label: string;
  volumePercent?: number;
  muted?: boolean;
  /** Only virtual devices have an addressable effect chain today — a stream
   * has no backend swap-by-identity mechanism to attach one to (see
   * `core/engine/effects_ops.rs`), so `deviceId` is omitted for streams and
   * the stage list simply doesn't render below Volume for them. */
  deviceId?: string;
}>();

const { pendingVolumes, clampVolume, scheduleChannelVolume, toggleChannelMute } = useMixerControls();

/**
 * Volume is backed directly by the node's own real device/stream volume
 * (`set_device_volume`/`set_stream_volume` — the same mechanism the flat
 * slider used before this component existed). This is deliberate: an
 * earlier per-connection gain mechanism broke routing (see issue #105's
 * follow-up) by inserting new PipeWire objects into the middle of a live
 * connection. Volume here touches zero topology.
 *
 * Volume is always present and pinned as the first row, not reorderable —
 * effect stages (PD-025, `EffectStageList` below) render underneath it and
 * are the things that reorder/remove, never Volume itself.
 */
const displayVolume = computed(() => pendingVolumes.value[props.entityId] ?? props.volumePercent ?? 0);

function onVolumeInput(event: Event) {
  const percent = Number((event.target as HTMLInputElement).value);
  scheduleChannelVolume(props.channelType, props.entityId, clampVolume(percent));
}

function onToggleMute() {
  void toggleChannelMute(props.channelType, props.entityId, Boolean(props.muted));
}
</script>

<template>
  <div class="routing-graph-node-effects nodrag">
    <div class="routing-graph-node-effect-row routing-graph-node-effect-row--pinned">
      <button
        type="button"
        class="routing-graph-node-mute"
        :class="{ active: muted }"
        :aria-label="muted ? 'Unmute' : 'Mute'"
        @click="onToggleMute"
      >
        {{ muted ? "🔇" : "🔊" }}
      </button>
      <input
        type="range"
        class="routing-graph-node-volume"
        min="0"
        max="100"
        :value="displayVolume"
        :aria-label="`${label} volume`"
        @input="onVolumeInput"
      />
      <span class="routing-graph-node-volume-label">{{ displayVolume }}%</span>
    </div>
    <EffectStageList v-if="deviceId" :device-id="deviceId" compact />
  </div>
</template>
