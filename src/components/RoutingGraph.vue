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
import RoutingGraphEdge from "./RoutingGraphEdge.vue";
import { useConnectionEffects } from "../composables/useConnectionEffects";
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
import { useNewDeviceDialog } from "../stores/newDeviceDialog";
import { usePrompt } from "../stores/prompt";
import { streamDisplayLabel } from "../utils/routingLayout";
import type { RuntimeGraph } from "../types/graph";

const props = defineProps<{
  graph: RuntimeGraph;
}>();

const { handleApplyResult } = useApplyResult();
const { confirm } = useConfirm();
const { prompt } = usePrompt();
const { openNewDeviceDialog } = useNewDeviceDialog();
const { addConnectionEffect, removeConnectionEffect } = useConnectionEffects();
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
  labelForEntity(entityId: string) {
    const stream = props.graph.streams.find((entry) => entry.id === entityId);
    if (stream) {
      return streamDisplayLabel(stream);
    }
    const device = props.graph.devices.find((entry) => entry.id === entityId);
    return device?.label ?? entityId;
  },
  outgoingConnectionsFor(entityId: string) {
    // A connection effect's target must be a device (a sink) — a capture
    // route's "target" is the capturing stream itself, which has no gain
    // mechanism on the backend. Only offer connections that end at a device.
    return props.graph.links
      .filter((link) => link.source_id === entityId && props.graph.devices.some((device) => device.id === link.target_id))
      .map((link) => ({
        sourceId: link.source_id,
        targetId: link.target_id,
        targetLabel: graphActions.labelForEntity(link.target_id),
        hasVolumeEffect: Boolean(link.effects?.some((effect) => effect.kind === "volume")),
      }));
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
  if (!target || target.kind !== "node" || !target.systemName) {
    return;
  }
  if (action === "rename") {
    void graphActions.renameDevice(target.systemName, target.label);
  } else if (action === "delete") {
    graphActions.deleteDevice(target.systemName, target.label);
  }
}

function onPaneContextMenu(event: MouseEvent) {
  event.preventDefault();
  contextMenu.value = { kind: "pane", x: event.clientX, y: event.clientY };
}

function onEdgeContextMenu(payload: EdgeMouseEvent) {
  const data = payload.edge.data as
    | { effects?: import("../types/graph").ConnectionEffectKind[]; rawSourceId: string; rawTargetId: string }
    | undefined;
  if (!data) return;

  // A connection effect's target must be a device (a sink) — a capture
  // route's "target" is the capturing stream itself, which the backend has
  // no gain mechanism for. Don't offer the menu item for those.
  const targetIsDevice = props.graph.devices.some((device) => device.id === data.rawTargetId);
  if (!targetIsDevice) return;

  payload.event.preventDefault();
  const hasVolumeEffect = Boolean(data.effects?.some((effect) => effect.kind === "volume"));
  contextMenu.value = {
    kind: "edge",
    x: (payload.event as MouseEvent).clientX,
    y: (payload.event as MouseEvent).clientY,
    sourceId: data.rawSourceId,
    targetId: data.rawTargetId,
    hasVolumeEffect,
  };
}

function onAddEffectAction(sourceId: string, targetId: string) {
  contextMenu.value = null;
  void addConnectionEffect(sourceId, targetId);
}

function onRemoveEffectAction(sourceId: string, targetId: string) {
  contextMenu.value = null;
  void removeConnectionEffect(sourceId, targetId);
}

function onAddNodeAction(type: "output" | "input") {
  contextMenu.value = null;
  openNewDeviceDialog(type);
}

const nodeTypes = {
  routingNode: markRaw(RoutingGraphNode),
  groupNode: markRaw(RoutingGraphGroupNode),
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
} as any;

const edgeTypes = {
  connectionEdge: markRaw(RoutingGraphEdge),
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
} as any;

const defaultEdgeOptions = {
  updatable: true,
  interactionWidth: 22,
} as const;

// Bumped whenever a drag persists a new position to the layout localStorage
// side channel, so `built` recomputes immediately instead of only on the next
// `graph-updated` push — otherwise a dragged node visually snaps back to its
// pre-drag spot the instant the drag ends (buildRoutingGraph reads the saved
// layout, but nothing here depends on it, so the computed cache is stale).
const layoutVersion = ref(0);
const built = computed(() => {
  layoutVersion.value;
  return buildRoutingGraph(props.graph, groups.value);
});
// Node ids currently mid-drag. `props.graph` can be replaced by a
// `graph-updated` push at any moment (mixer/routing/rule commands and the
// live PipeWire monitor all emit it, often several times a second) — without
// this, a rebuild landing mid-drag recomputes positions from the *last
// saved* layout (only updated on drag-stop) and snaps the dragged node back
// to its pre-drag spot while vue-flow's own connection-line cursor tracking
// keeps going, splitting the two visually. While a node id is in this set,
// its live vue-flow position is preserved instead of the freshly built one.
const draggingNodeIds = ref<Set<string>>(new Set());
const nodes = computed<Node[]>(() => {
  if (draggingNodeIds.value.size === 0) {
    return built.value.nodes as Node[];
  }
  return (built.value.nodes as Node[]).map((node) => {
    if (!draggingNodeIds.value.has(node.id)) {
      return node;
    }
    const live = vueFlow.findNode(node.id);
    return live ? { ...node, position: live.computedPosition } : node;
  });
});
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
  // Vue Flow reuses this callback both for a live user drag (a bare Connection,
  // no `id`) and to re-validate every already-persisted edge on each resync
  // (which carries its own `id`). Only the former should require the target to
  // be the open trailing slot.
  const isExistingEdge = Boolean((connection as unknown as { id?: string }).id);
  return canConnectPorts(connection.sourceHandle, connection.targetHandle, !isExistingEdge);
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

function onNodeDragStart(event: NodeDragEvent) {
  // event.nodes carries every node vue-flow is moving together in this drag
  // (a multi-select drag); fall back to just event.node when it's
  // undefined/empty (single-node drag).
  const dragged = event.nodes?.length ? event.nodes : [event.node];
  const next = new Set(draggingNodeIds.value);
  for (const draggedNode of dragged) {
    next.add(draggedNode.id);
  }
  draggingNodeIds.value = next;
}

function onNodeDragStop(event: NodeDragEvent) {
  const node = event.node;
  const idsToClear = event.nodes?.length ? event.nodes.map((n) => n.id) : [node.id];
  const next = new Set(draggingNodeIds.value);
  for (const id of idsToClear) {
    next.delete(id);
  }
  draggingNodeIds.value = next;

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
      layoutVersion.value += 1;
      persistGroups();
    }
    return;
  }

  // Non-group node(s). event.nodes carries every node vue-flow moved together
  // during this drag (multi-select); fall back to just node for a single drag.
  const draggedNodes = event.nodes?.length ? event.nodes : [node];
  for (const draggedNode of draggedNodes) {
    saveNodePosition(draggedNode.id, draggedNode.computedPosition.x, draggedNode.computedPosition.y);
  }
  layoutVersion.value += 1;

  // Detach-from-group check only considers the primary grabbed node — a node
  // landing outside its group's bounds as a side effect of where other
  // multi-selected nodes were relative to the pointer shouldn't silently
  // detach it on its own.
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

// Nodes carry a variable number of handles (one per live connection, plus a
// trailing empty slot) that changes as routing changes. Vue Flow caches each
// handle's rendered position and only recomputes it on an explicit nudge, so
// without this, edges/arrows draw at stale coordinates until something else
// (e.g. a window resize) forces a recalculation.
watch(
  nodes,
  async (current) => {
    await nextTick();
    vueFlow.updateNodeInternals(current.map((node) => node.id));

    const currentIds = new Set(current.map((node) => node.id));
    if (knownNodeIds.value === null) {
      knownNodeIds.value = currentIds;
      return;
    }

    const addedIds = [...currentIds].filter((id) => !knownNodeIds.value!.has(id));
    knownNodeIds.value = currentIds;
    if (addedIds.length === 0) return;

    await vueFlow.fitView({ nodes: addedIds, padding: 0.35, duration: 400, maxZoom: 1 });
  },
);

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
          Drag wire ends off a port to disconnect · Shift+drag to select multiple nodes · Press G to group ·
          Right-click empty space to add a node
        </span>
      </div>
    </div>
    <RoutingGraphContextMenu
      :target="contextMenu"
      @rename="onContextMenuAction('rename')"
      @delete="onContextMenuAction('delete')"
      @add-node="onAddNodeAction"
      @add-effect="onAddEffectAction"
      @remove-effect="onRemoveEffectAction"
      @close="contextMenu = null"
    />
    <div class="routing-graph-canvas">
      <VueFlow
        :nodes="nodes"
        :edges="edges"
        :node-types="nodeTypes"
        :edge-types="edgeTypes"
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
        @edge-context-menu="onEdgeContextMenu"
        @node-drag-start="onNodeDragStart"
        @node-drag-stop="onNodeDragStop"
        @pane-click="onPaneClick"
        @pane-context-menu="onPaneContextMenu"
      >
        <Background pattern-color="rgba(255,255,255,0.04)" :gap="20" />
        <Controls />
      </VueFlow>
    </div>
  </div>
</template>
