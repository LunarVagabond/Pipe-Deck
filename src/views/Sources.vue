<script setup lang="ts">
import { computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import NodeCardHeader from "../components/NodeCardHeader.vue";
import StreamTargetPicker from "../components/StreamTargetPicker.vue";
import { useApplyResult } from "../stores/notices";
import { useConfirm } from "../stores/confirm";
import { useRuntimeGraph } from "../stores/runtimeGraph";
import type { Device } from "../types/graph";
import {
  deviceSubtitle,
  isVirtualMicDevice,
  targetLabel,
  targetsForVirtualSink,
  virtualMicFeedSinks,
} from "../utils/routingLayout";

const { graph, loading, error, refresh } = useRuntimeGraph();
const { handleApplyResult } = useApplyResult();
const { confirm } = useConfirm();

const isMockData = computed(() => graph.value.data_source === "mock");

const captureStreams = computed(() =>
  graph.value.streams.filter((stream) => stream.direction === "capture"),
);

const inputDevices = computed(() =>
  graph.value.devices.filter(
    (device) =>
      (device.direction === "input" || device.direction === "duplex") &&
      !device.system_name.startsWith("pipe-deck-feed-"),
  ),
);

const virtualMics = computed(() => inputDevices.value.filter(isVirtualMicDevice));

const isEmpty = computed(
  () =>
    !loading.value &&
    !error.value &&
    captureStreams.value.length === 0 &&
    inputDevices.value.length === 0,
);

function feedSourcesForMic(virtualMic: Device): Device[] {
  return graph.value.devices.filter((device) => {
    if (device.id === virtualMic.id) return false;
    if (device.direction !== "input" && device.direction !== "duplex") return false;
    return graph.value.links.some(
      (link) => link.source_id === device.id && link.target_id === virtualMic.id,
    );
  });
}

async function onDeviceRouteChange(sourceDeviceId: string, event: Event) {
  const targetDeviceId = (event.target as HTMLSelectElement).value;
  if (!targetDeviceId) return;
  try {
    const result = await invoke<{ success: boolean; message?: string }>("set_device_route", {
      sourceDeviceId,
      targetDeviceId,
    });
    handleApplyResult(result, "Virtual mic route updated");
  } catch (err) {
    handleApplyResult(
      { success: false, message: err instanceof Error ? err.message : String(err) },
      "",
    );
  }
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
    <header class="sources-header">
      <div>
        <p class="eyebrow">Capture and inputs</p>
        <h1>Sources</h1>
      </div>
      <div class="sources-actions">
        <button type="button" @click="refresh">Refresh</button>
      </div>
    </header>

    <p v-if="isMockData" class="notice-banner mock">
      {{ graph.notice ?? "Showing sample data (PIPE_DECK_USE_MOCK=1)." }}
    </p>
    <p v-else-if="graph.notice" class="notice-banner warn">
      {{ graph.notice }}
    </p>

    <p v-if="loading" class="status">Loading runtime graph…</p>
    <p v-else-if="error" class="status error">{{ error }}</p>
    <p v-else-if="isEmpty" class="status">No capture streams or input devices detected.</p>

    <template v-else>
      <section class="sources-section">
        <h2>Capture streams</h2>
        <p v-if="captureStreams.length === 0" class="empty">No application capture streams active.</p>
        <div v-else class="sources-card-list">
          <StreamTargetPicker
            v-for="stream in captureStreams"
            :key="stream.id"
            :stream="stream"
            :devices="graph.devices"
          />
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
              <p
                v-for="source in feedSourcesForMic(device)"
                :key="source.id"
                class="sources-feed-note"
              >
                Fed by {{ source.label }}
              </p>
            </div>
          </article>
        </div>
      </section>

      <section v-if="virtualMics.length > 0" class="sources-section">
        <h2>Virtual microphone routes</h2>
        <p class="section-help">
          Virtual sinks routed to a virtual input become shareable microphones for capture apps.
        </p>
        <div class="sources-device-grid">
          <article
            v-for="virtualMic in virtualMics"
            :key="virtualMic.id"
            class="sources-device-card sources-route-card"
          >
            <span class="node-icon input">🎙</span>
            <div class="sources-device-body">
              <strong>{{ targetLabel(virtualMic) }}</strong>
              <span class="node-sub">{{ deviceSubtitle(virtualMic) }}</span>
              <div
                v-for="sink in virtualMicFeedSinks(graph.devices, virtualMic)"
                :key="sink.id"
                class="routing-picker capture"
              >
                <span class="routing-label">{{ sink.label }} → route to</span>
                <select
                  class="routing-select"
                  :value="sink.current_target ?? ''"
                  @change="onDeviceRouteChange(sink.id, $event)"
                >
                  <option value="" disabled>Select target</option>
                  <option
                    v-for="target in targetsForVirtualSink(graph.devices, sink)"
                    :key="target.id"
                    :value="target.id"
                  >
                    {{ targetLabel(target) }}
                  </option>
                </select>
              </div>
              <p
                v-if="virtualMicFeedSinks(graph.devices, virtualMic).length === 0"
                class="sources-feed-note muted"
              >
                No virtual sink routes to this microphone yet.
              </p>
            </div>
          </article>
        </div>
      </section>
    </template>
  </div>
</template>
