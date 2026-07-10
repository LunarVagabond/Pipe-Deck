<script setup lang="ts">
import { computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import RouteExplanationPanel from "./RouteExplanationPanel.vue";
import { useApplyResult } from "../stores/notices";
import type { Device, Stream } from "../types/graph";
import {
  sinksForStream,
  streamAccent,
  streamSubtitle,
  targetLabel,
} from "../utils/routingLayout";

const props = defineProps<{
  stream: Stream;
  devices: Device[];
}>();

const { handleApplyResult } = useApplyResult();

const targets = computed(() => sinksForStream(props.devices, props.stream));

async function onTargetChange(event: Event) {
  const targetDeviceId = (event.target as HTMLSelectElement).value;
  if (!targetDeviceId) return;
  try {
    const result = await invoke<{ success: boolean; message?: string }>("set_stream_target", {
      streamId: props.stream.id,
      targetDeviceId,
    });
    handleApplyResult(result, "Routing updated");
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}
</script>

<template>
  <div
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
