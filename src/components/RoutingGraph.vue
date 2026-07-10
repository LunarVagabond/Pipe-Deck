<script setup lang="ts">
import { computed, markRaw, onMounted, onUnmounted, provide, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  VueFlow,
  MarkerType,
  type Connection,
  type Edge,
  type EdgeMouseEvent,
  type EdgeUpdateEvent,
  type Node,
  type NodeDragEvent,
} from "@vue-flow/core";
import { Background } from "@vue-flow/background";
import { Controls } from "@vue-flow/controls";
import RoutingGraphContextMenu from "./RoutingGraphContextMenu.vue";
import RoutingGraphNode from "./RoutingGraphNode.vue";
import {
  applyEdgeDisconnect,
  applyRoutingConnection,
} from "./routing-graph/applyConnection";
import { buildRoutingGraph, saveNodePosition } from "./routing-graph/buildGraph";
import { LEGEND_ENTRIES } from "./routing-graph/portTypes";
import { canConnectPorts } from "./routing-graph/portTypes";
import {
  routingGraphActionsKey,
  type RoutingGraphMenuTarget,
} from "../composables/routingGraphContext";
import { useApplyResult } from "../stores/notices";
import { useConfirm } from "../stores/confirm";
import { usePrompt } from "../stores/prompt";
import type { RuntimeGraph } from "../types/graph";

const props = defineProps<{
  graph: RuntimeGraph;
}>();

const { handleApplyResult } = useApplyResult();
const { confirm } = useConfirm();
const { prompt } = usePrompt();

const edgeUpdatePending = ref<Edge | null>(null);
const contextMenu = ref<RoutingGraphMenuTarget | null>(null);

const graphActions = {
  openMenu(target: RoutingGraphMenuTarget) {
    contextMenu.value = target;
  },
  closeMenu() {
    contextMenu.value = null;
  },
  async renameDevice(systemName: string, currentLabel: string, alias?: string) {
    contextMenu.value = null;
    const next =
      alias ??
      (await prompt({
        title: "Rename device",
        defaultValue: currentLabel,
        confirmLabel: "Save",
      }));
    if (!next) {
      return;
    }
    const trimmed = next.trim();
    if (!trimmed || trimmed === currentLabel) {
      return;
    }
    void saveDeviceAlias(systemName, trimmed);
  },
  deleteDevice(systemName: string, label: string) {
    contextMenu.value = null;
    void removeVirtualDevice(systemName, label);
  },
};

provide(routingGraphActionsKey, graphActions);

async function saveDeviceAlias(systemName: string, alias: string) {
  try {
    await invoke("set_device_alias", { systemName, alias });
    handleApplyResult({ success: true }, "Device renamed");
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

async function removeVirtualDevice(systemName: string, label: string) {
  const confirmed = await confirm(`Delete virtual device "${label}"?`, {
    title: "Delete virtual device",
    confirmLabel: "Delete",
    cancelLabel: "Cancel",
  });
  if (!confirmed) {
    return;
  }

  try {
    await invoke("remove_virtual_device", { systemName });
    handleApplyResult({ success: true }, "Virtual device removed");
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

function onContextMenuAction(action: "rename" | "delete") {
  const target = contextMenu.value;
  if (!target) {
    return;
  }
  if (action === "rename") {
    void graphActions.renameDevice(target.systemName, target.label);
  } else if (action === "delete") {
    graphActions.deleteDevice(target.systemName, target.label);
  }
}

const nodeTypes = {
  routingNode: markRaw(RoutingGraphNode),
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
} as any;

const defaultEdgeOptions = {
  updatable: true,
  interactionWidth: 22,
} as const;

const built = computed(() => buildRoutingGraph(props.graph));
const nodes = computed<Node[]>(() => built.value.nodes as Node[]);
const edges = computed<Edge[]>(() =>
  built.value.edges.map((edge) => ({
    ...edge,
    markerEnd: {
      type: MarkerType.ArrowClosed,
      color: edge.style?.stroke ?? "#7c5cff",
      width: 18,
      height: 18,
    },
  })) as Edge[],
);

const legend = LEGEND_ENTRIES;

watch(
  () => props.graph,
  () => {
    contextMenu.value = null;
  },
  { deep: true },
);

function isValidConnection(connection: Connection) {
  return canConnectPorts(connection.sourceHandle, connection.targetHandle);
}

async function commitConnection(
  connection: Connection,
  mode: "connect" | "edge_update" = "connect",
  previousEdge?: Edge,
) {
  await applyRoutingConnection(
    props.graph,
    connection,
    handleApplyResult,
    mode === "edge_update" && previousEdge
      ? { mode: "edge_update", previousEdge }
      : { mode: "connect" },
  );
}

async function onConnect(connection: Connection) {
  await commitConnection(connection, "connect");
}

async function onEdgeUpdate(event: EdgeUpdateEvent) {
  edgeUpdatePending.value = null;
  await commitConnection(event.connection, "edge_update", event.edge);
}

function onEdgeUpdateStart(event: EdgeMouseEvent) {
  edgeUpdatePending.value = event.edge;
}

async function onEdgeUpdateEnd() {
  const pending = edgeUpdatePending.value;
  edgeUpdatePending.value = null;
  if (!pending) {
    return;
  }
  await applyEdgeDisconnect(
    props.graph,
    {
      source: pending.source,
      target: pending.target,
      sourceHandle: pending.sourceHandle,
      targetHandle: pending.targetHandle,
    },
    handleApplyResult,
  );
}

function onPaneClick() {
  contextMenu.value = null;
}

function onDocumentPointerDown(event: PointerEvent) {
  const target = event.target;
  if (target instanceof Element && target.closest(".routing-graph-context-menu")) {
    return;
  }
  contextMenu.value = null;
}

function onNodeDragStop(event: NodeDragEvent) {
  saveNodePosition(event.node.id, event.node.position.x, event.node.position.y);
}

onMounted(() => {
  localStorage.removeItem("pipe-deck-routing-reroutes");
  window.addEventListener("pointerdown", onDocumentPointerDown);
});

onUnmounted(() => {
  window.removeEventListener("pointerdown", onDocumentPointerDown);
});
</script>

<template>
  <div class="routing-graph-shell">
    <div class="routing-graph-legend" aria-label="Connection color legend">
      <span class="routing-graph-legend-title">Output connects to input</span>
      <div class="routing-graph-legend-items">
        <span v-for="entry in legend" :key="entry.key" class="routing-graph-legend-item">
          <span class="routing-graph-legend-swatch" :style="{ background: entry.color }" />
          {{ entry.label }}
        </span>
        <span class="routing-graph-legend-hint">
          Drag wire ends off a port to disconnect
        </span>
      </div>
    </div>
    <RoutingGraphContextMenu
      :target="contextMenu"
      @rename="onContextMenuAction('rename')"
      @delete="onContextMenuAction('delete')"
      @close="contextMenu = null"
    />
    <div class="routing-graph-canvas">
      <VueFlow
        :nodes="nodes"
        :edges="edges"
        :node-types="nodeTypes"
        :default-edge-options="defaultEdgeOptions"
        :edges-updatable="true"
        :edge-updater-radius="12"
        :fit-view-on-init="true"
        :min-zoom="0.35"
        :max-zoom="1.5"
        :is-valid-connection="isValidConnection"
        @connect="onConnect"
        @edge-update="onEdgeUpdate"
        @edge-update-start="onEdgeUpdateStart"
        @edge-update-end="onEdgeUpdateEnd"
        @node-drag-stop="onNodeDragStop"
        @pane-click="onPaneClick"
      >
        <Background pattern-color="rgba(255,255,255,0.04)" :gap="20" />
        <Controls />
      </VueFlow>
    </div>
  </div>
</template>
