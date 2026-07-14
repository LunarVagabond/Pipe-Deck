<script setup lang="ts">
import { computed, inject } from "vue";
import { Handle, Position } from "@vue-flow/core";
import NodeCardHeader from "./NodeCardHeader.vue";
import NodeTypeIcon from "./NodeTypeIcon.vue";
import type { RoutingGraphHandle, RoutingGraphNodeData } from "./routing-graph/buildGraph";
import { useMixerControls } from "../composables/useMixerControls";
import { routingGraphActionsKey } from "../composables/routingGraphContext";

const props = defineProps<{
  data: RoutingGraphNodeData;
}>();

const actions = inject(routingGraphActionsKey, null);
const { pendingVolumes, clampVolume, scheduleChannelVolume, toggleChannelMute } =
  useMixerControls();

const inHandles = computed(() => props.data.handles.filter((handle) => handle.position === "left"));
const outHandles = computed(() =>
  props.data.handles.filter((handle) => handle.position === "right"),
);

function portTitle(handle: RoutingGraphHandle): string {
  if (handle.empty) {
    return "Not connected — drag here to connect";
  }
  if (handle.connectedId) {
    return actions?.labelForEntity(handle.connectedId) ?? "Connected";
  }
  return "";
}

function onContextMenu(event: MouseEvent) {
  const canRenameOrDelete = Boolean(props.data.systemName) && (props.data.editable || props.data.deletable);
  const connections = actions?.outgoingConnectionsFor(props.data.entityId) ?? [];
  if (!canRenameOrDelete && connections.length === 0) {
    return;
  }
  event.preventDefault();
  event.stopPropagation();
  actions?.openMenu({
    kind: "node",
    x: event.clientX,
    y: event.clientY,
    label: props.data.label,
    systemName: props.data.systemName,
    editable: Boolean(props.data.editable),
    deletable: Boolean(props.data.deletable),
    connections,
  });
}

function onRename(alias: string) {
  if (!props.data.systemName) return;
  actions?.renameDevice(props.data.systemName, props.data.label, alias);
}

function onDelete() {
  if (!props.data.systemName) return;
  actions?.deleteDevice(props.data.systemName, props.data.label);
}

const displayVolume = computed(() => {
  if (!props.data.channelType) return 0;
  return pendingVolumes.value[props.data.entityId] ?? props.data.volumePercent ?? 0;
});

function onVolumeInput(event: Event) {
  if (!props.data.channelType) return;
  const percent = Number((event.target as HTMLInputElement).value);
  scheduleChannelVolume(props.data.channelType, props.data.entityId, clampVolume(percent));
}

function onToggleMute() {
  if (!props.data.channelType) return;
  void toggleChannelMute(props.data.channelType, props.data.entityId, Boolean(props.data.muted));
}
</script>

<template>
  <div class="routing-graph-node nopan" :class="data.nodeClass" @contextmenu="onContextMenu">
    <div v-if="inHandles.length" class="routing-graph-node-ports routing-graph-node-ports--in">
      <div
        v-for="handle in inHandles"
        :key="handle.id"
        class="routing-graph-port-row"
        :class="{ 'is-empty': handle.empty }"
        :title="portTitle(handle)"
      >
        <Handle
          :id="handle.id"
          type="target"
          :position="Position.Left"
          class="routing-graph-handle"
          :class="{ 'is-empty': handle.empty }"
        />
      </div>
    </div>

    <div class="routing-graph-node-main">
      <div class="routing-graph-node-body">
        <span
          v-if="data.accent"
          class="routing-graph-node-swatch"
          :style="{ background: data.accent }"
        />
        <NodeTypeIcon :kind="data.nodeClass" class="routing-graph-node-icon" />
        <div class="routing-graph-node-copy">
          <NodeCardHeader
            v-if="data.systemName"
            :label="data.label"
            :editable="data.editable"
            :deletable="data.deletable"
            layout="inline"
            @save="onRename"
            @delete="onDelete"
          />
          <strong v-else>{{ data.label }}</strong>
          <span class="routing-graph-node-sub">{{ data.subtitle }}</span>
        </div>
      </div>

      <div v-if="data.channelType" class="routing-graph-node-mixer nodrag">
        <button
          type="button"
          class="routing-graph-node-mute"
          :class="{ active: data.muted }"
          :aria-label="data.muted ? 'Unmute' : 'Mute'"
          @click="onToggleMute"
        >
          {{ data.muted ? "🔇" : "🔊" }}
        </button>
        <input
          type="range"
          class="routing-graph-node-volume"
          min="0"
          max="100"
          :value="displayVolume"
          :aria-label="`${data.label} volume`"
          @input="onVolumeInput"
        />
        <span class="routing-graph-node-volume-label">{{ displayVolume }}%</span>
      </div>
    </div>

    <div v-if="outHandles.length" class="routing-graph-node-ports routing-graph-node-ports--out">
      <div
        v-for="handle in outHandles"
        :key="handle.id"
        class="routing-graph-port-row"
        :class="{ 'is-empty': handle.empty }"
        :title="portTitle(handle)"
      >
        <Handle
          :id="handle.id"
          type="source"
          :position="Position.Right"
          class="routing-graph-handle"
          :class="{ 'is-empty': handle.empty }"
        />
      </div>
    </div>
  </div>
</template>
