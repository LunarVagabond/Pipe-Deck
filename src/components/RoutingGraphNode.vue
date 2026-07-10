<script setup lang="ts">
import { inject } from "vue";
import { Handle, Position } from "@vue-flow/core";
import NodeCardHeader from "./NodeCardHeader.vue";
import type { RoutingGraphHandle, RoutingGraphNodeData } from "./routing-graph/buildGraph";
import { PORT_META } from "./routing-graph/portTypes";
import { routingGraphActionsKey } from "../composables/routingGraphContext";

const props = defineProps<{
  data: RoutingGraphNodeData;
}>();

const actions = inject(routingGraphActionsKey, null);

function handlePosition(side: "left" | "right"): Position {
  return side === "left" ? Position.Left : Position.Right;
}

function handleStyle(handle: RoutingGraphHandle) {
  return {
    "--handle-color": PORT_META[handle.id].color,
  };
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
</script>

<template>
  <div class="routing-graph-node nopan" :class="data.nodeClass" @contextmenu="onContextMenu">
    <Handle
      v-for="handle in data.handles"
      :id="handle.id"
      :key="handle.id"
      :type="handle.type"
      :position="handlePosition(handle.position)"
      class="routing-graph-handle"
      :class="`routing-graph-handle--${handle.id}`"
      :style="handleStyle(handle)"
    />
    <div class="routing-graph-node-body">
      <span
        v-if="data.accent"
        class="routing-graph-node-swatch"
        :style="{ background: data.accent }"
      />
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
  </div>
</template>
