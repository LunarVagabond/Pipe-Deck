<script setup lang="ts">
import { computed } from "vue";
import RouteExplanationPanel from "./RouteExplanationPanel.vue";
import { useRoutingActions } from "../composables/useRoutingActions";
import type { Device, Stream } from "../types/graph";
import {
  sinksForStream,
  streamAccent,
  streamSubtitle,
  targetLabel,
} from "../utils/routingLayout";

const props = withDefaults(
  defineProps<{
    stream: Stream;
    devices: Device[];
    compact?: boolean;
  }>(),
  { compact: false },
);

const { setStreamTarget } = useRoutingActions();

const targets = computed(() => sinksForStream(props.devices, props.stream));

async function onTargetChange(event: Event) {
  const targetDeviceId = (event.target as HTMLSelectElement).value;
  if (!targetDeviceId) return;
  await setStreamTarget(props.stream.id, targetDeviceId);
}
</script>

<template>
  <div
    v-if="compact"
    class="stream-target-picker stream-target-picker--compact"
    :class="stream.direction === 'capture' ? 'capture' : 'playback'"
  >
    <select
      class="routing-select"
      aria-label="Change routing target"
      :value="stream.current_target ?? ''"
      @change="onTargetChange"
    >
      <option value="" disabled>Select target</option>
      <option v-for="target in targets" :key="target.id" :value="target.id">
        {{ targetLabel(target) }}
      </option>
    </select>
  </div>
  <div
    v-else
    class="stream-target-picker"
    :class="stream.direction === 'capture' ? 'capture' : 'playback'"
  >
    <span class="node-icon" :style="{ background: streamAccent(stream.id) }">
      {{ stream.app_name.charAt(0) }}
    </span>
    <div class="stream-target-body">
      <strong class="stream-title">{{ stream.app_name }}</strong>
      <span class="node-sub">{{ streamSubtitle(stream) }}</span>
      <div class="routing-picker">
        <span class="routing-label">
          {{ stream.direction === "capture" ? "Record from" : "Route to" }}
        </span>
        <select
          class="routing-select"
          aria-label="Change routing target"
          :value="stream.current_target ?? ''"
          @change="onTargetChange"
        >
          <option value="" disabled>Select target</option>
          <option v-for="target in targets" :key="target.id" :value="target.id">
            {{ targetLabel(target) }}
          </option>
        </select>
      </div>
      <RouteExplanationPanel :stream="stream" :devices="devices" />
    </div>
  </div>
</template>
