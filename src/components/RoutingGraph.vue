<script setup lang="ts">
import { computed, markRaw, nextTick, onMounted, onUnmounted, provide, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  VueFlow,
  useVueFlow,
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
import RoutingGraphGroupNode from "./RoutingGraphGroupNode.vue";
import {
  applyEdgeDisconnect,
  applyRoutingConnection,
} from "./routing-graph/applyConnection";
import { buildRoutingGraph, saveNodePosition } from "./routing-graph/buildGraph";
import { LEGEND_ENTRIES } from "./routing-graph/portTypes";
import { canConnectPorts } from "./routing-graph/portTypes";
import {
  containmentRatio,
  createGroup,
  loadGroups,
  saveGroups,
  type GraphGroup,
} from "./routing-graph/groups";
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
const vueFlow = useVueFlow();

const edgeUpdatePending = ref<Edge | null>(null);
const contextMenu = ref<RoutingGraphMenuTarget | null>(null);
const groups = ref<GraphGroup[]>(loadGroups());

function persistGroups() {
  saveGroups(groups.value);
}

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
  renameGroup(groupId: string, label: string) {
    const group = groups.value.find((entry) => entry.id === groupId);
    if (!group) return;
    group.label = label;
    persistGroups();
  },
  ungroup(groupId: string) {
    groups.value = groups.value.filter((entry) => entry.id !== groupId);
    persistGroups();
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
  groupNode: markRaw(RoutingGraphGroupNode),
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
} as any;

const defaultEdgeOptions = {
  updatable: true,
  interactionWidth: 22,
} as const;

const built = computed(() => buildRoutingGraph(props.graph, groups.value));
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

const DETACH_THRESHOLD = 0.4;

function onNodeDragStop(event: NodeDragEvent) {
  const node = event.node;

  if (node.type === "groupNode") {
    const group = groups.value.find((entry) => entry.id === node.id);
    if (group) {
      group.position = { x: node.computedPosition.x, y: node.computedPosition.y };
      // Vue Flow already moved member nodes along with the group during the drag (they
      // track it live via parentNode). Persist each member's up-to-date absolute position
      // now, otherwise the next rebuild recomputes their offset from stale saved
      // coordinates and they visually snap back to their pre-drag spot.
      for (const memberId of group.memberIds) {
        const memberNode = vueFlow.findNode(memberId);
        if (memberNode) {
          saveNodePosition(memberId, memberNode.computedPosition.x, memberNode.computedPosition.y);
        }
      }
      persistGroups();
    }
    return;
  }

  saveNodePosition(node.id, node.computedPosition.x, node.computedPosition.y);

  const group = groups.value.find((entry) => entry.memberIds.includes(node.id));
  if (!group) return;

  const nodeRect = {
    x: node.computedPosition.x,
    y: node.computedPosition.y,
    width: node.dimensions.width,
    height: node.dimensions.height,
  };
  const groupRect = {
    x: group.position.x,
    y: group.position.y,
    width: group.size.width,
    height: group.size.height,
  };

  if (containmentRatio(nodeRect, groupRect) < DETACH_THRESHOLD) {
    group.memberIds = group.memberIds.filter((id) => id !== node.id);
    if (group.memberIds.length === 0) {
      groups.value = groups.value.filter((entry) => entry.id !== group.id);
    }
    persistGroups();
  }
}

const MIN_GROUP_SELECTION = 2;

function isTypingTarget(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && ["INPUT", "TEXTAREA"].includes(target.tagName);
}

async function onWindowKeydown(event: KeyboardEvent) {
  if (event.key.toLowerCase() !== "g" || event.metaKey || event.ctrlKey || event.altKey) {
    return;
  }
  if (isTypingTarget(event.target)) return;

  const selected = vueFlow.getSelectedNodes.value.filter(
    (candidate) => candidate.type !== "groupNode" && !candidate.parentNode,
  );
  if (selected.length < MIN_GROUP_SELECTION) return;

  event.preventDefault();
  const name = await prompt({
    title: "Name this group",
    defaultValue: "Group",
    confirmLabel: "Create",
  });
  const trimmed = name?.trim();
  if (!trimmed) return;

  const group = createGroup(
    trimmed,
    selected.map((candidate) => ({
      id: candidate.id,
      position: candidate.computedPosition,
      width: candidate.dimensions.width || 200,
      height: candidate.dimensions.height || 80,
    })),
  );
  groups.value = [...groups.value, group];
  persistGroups();
}

const knownNodeIds = ref<Set<string> | null>(null);

watch(nodes, async (current) => {
  const currentIds = new Set(current.map((node) => node.id));
  if (knownNodeIds.value === null) {
    knownNodeIds.value = currentIds;
    return;
  }

  const addedIds = [...currentIds].filter((id) => !knownNodeIds.value!.has(id));
  knownNodeIds.value = currentIds;
  if (addedIds.length === 0) return;

  await nextTick();
  await vueFlow.fitView({ nodes: addedIds, padding: 0.35, duration: 400, maxZoom: 1 });
});

onMounted(() => {
  localStorage.removeItem("pipe-deck-routing-reroutes");
  window.addEventListener("pointerdown", onDocumentPointerDown);
  window.addEventListener("keydown", onWindowKeydown);
});

onUnmounted(() => {
  window.removeEventListener("pointerdown", onDocumentPointerDown);
  window.removeEventListener("keydown", onWindowKeydown);
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
          Drag wire ends off a port to disconnect · Shift+drag to select multiple nodes · Press G to group
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
