<script setup lang="ts">
import { computed, inject } from "vue";
import { Handle, Position, useNodeId } from "@vue-flow/core";
import NodeCardHeader from "./NodeCardHeader.vue";
import NodeTypeIcon from "./NodeTypeIcon.vue";
import RoutingGraphNodeEffects from "./RoutingGraphNodeEffects.vue";
import type { RoutingGraphHandle, RoutingGraphNodeData } from "./routing-graph/buildGraph";
import { useMixerControls } from "../composables/useMixerControls";
import { routingGraphActionsKey } from "../composables/routingGraphContext";

const props = defineProps<{
  data: RoutingGraphNodeData;
}>();

const actions = inject(routingGraphActionsKey, null);
const nodeId = useNodeId();
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

/** Screen-reader label for a port: what it is, and what (if anything) it's
 * wired to today — the sighted view conveys the same via `portTitle`'s
 * hover tooltip plus the port's filled/empty styling. */
function handleAriaLabel(handle: RoutingGraphHandle): string {
  const direction = handle.type === "source" ? "output" : "input";
  if (handle.empty) {
    return `${props.data.label} ${direction} port, not connected`;
  }
  const other = handle.connectedId ? actions?.labelForEntity(handle.connectedId) : undefined;
  return `${props.data.label} ${direction} port, connected to ${other ?? "another device"}`;
}

/** Enter/Space triggers the same click Vue Flow's own click-to-connect
 * handling already listens for (see useHandle in @vue-flow/core) — reusing
 * that state machine instead of re-implementing connect validation here.
 * Delete/Backspace on an occupied port is the keyboard equivalent of
 * dragging a wire end off to disconnect it. */
function onHandleKeydown(event: KeyboardEvent, handle: RoutingGraphHandle) {
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    (event.currentTarget as HTMLElement).click();
    return;
  }
  if (event.key === "Delete" || event.key === "Backspace") {
    if (handle.empty || !handle.connectedId || !nodeId) return;
    event.preventDefault();
    void actions?.disconnectPort(nodeId, handle);
  }
}

function onContextMenu(event: MouseEvent) {
  if (!props.data.systemName || (!props.data.editable && !props.data.deletable)) {
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

// Plain volume control for hardware (physical) devices — no effects list,
// no drag handle, just the bare slider that's been here since before any of
// the effects work (see RoutingGraphNodeEffects.vue for the virtual/stream case).
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
          tabindex="0"
          role="button"
          :aria-label="handleAriaLabel(handle)"
          @keydown="(event) => onHandleKeydown(event, handle)"
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

      <RoutingGraphNodeEffects
        v-if="data.channelType && data.supportsEffects"
        :channel-type="data.channelType"
        :entity-id="data.entityId"
        :label="data.label"
        :volume-percent="data.volumePercent"
        :muted="data.muted"
      />
      <div v-else-if="data.channelType" class="routing-graph-node-mixer nodrag">
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
          tabindex="0"
          role="button"
          :aria-label="handleAriaLabel(handle)"
          @keydown="(event) => onHandleKeydown(event, handle)"
        />
      </div>
    </div>
  </div>
</template>
