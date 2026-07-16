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
import { nodeIdsForLink } from "./routing-graph/connectionRules";
import { buildRoutingGraph, parseGraphNodeId, saveNodePosition } from "./routing-graph/buildGraph";
import type { RoutingGraphHandle } from "./routing-graph/buildGraph";
import { LEGEND_ENTRIES } from "./routing-graph/portTypes";
import { canConnectPorts } from "./routing-graph/portTypes";
import {
  boundsForMembers,
  containmentRatio,
  createGroup,
  loadGroups,
  MEMBER_GAP,
  nearestGroupEdge,
  reflowMembers,
  saveGroups,
  type GraphGroup,
  type GraphRect,
  type GroupEdge,
  type GroupLayoutAxis,
  type GroupMemberInput,
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
  setGroupColor(groupId: string, color: string) {
    const group = groups.value.find((entry) => entry.id === groupId);
    if (!group) return;
    group.color = color;
    persistGroups();
  },
  ungroup(groupId: string) {
    const group = groups.value.find((entry) => entry.id === groupId);
    if (group) {
      // Members are rendered with a `parentNode`-relative position while
      // grouped; their saved layout entry may predate grouping or never have
      // existed (auto-placed nodes). Persist each member's live absolute
      // position before dropping `parentNode`, otherwise buildRoutingGraph
      // falls back to disconnected auto-placed lane slots on the next
      // render and the node visually jumps.
      for (const memberId of group.memberIds) {
        const memberNode = vueFlow.findNode(memberId);
        if (memberNode) {
          saveNodePosition(memberId, memberNode.computedPosition.x, memberNode.computedPosition.y);
        }
      }
    }
    groups.value = groups.value.filter((entry) => entry.id !== groupId);
    layoutVersion.value += 1;
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
  async disconnectPort(nodeId: string, handle: RoutingGraphHandle) {
    if (handle.empty || !handle.connectedId) return;
    const parsed = parseGraphNodeId(nodeId);
    if (!parsed) return;

    const { source, target } =
      handle.type === "source"
        ? { source: parsed.id, target: handle.connectedId }
        : { source: handle.connectedId, target: parsed.id };
    const ids = nodeIdsForLink(props.graph, source, target);
    await applyEdgeDisconnect(props.graph, { source: ids.source, target: ids.target }, handleApplyResult);
  },
};

provide(routingGraphActionsKey, graphActions);

async function saveDeviceAlias(systemName: string, alias: string) {
  try {
    await invoke("set_device_alias", { systemName, alias });
    handleApplyResult({ success: true }, "Device renamed");
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    handleApplyResult(
      { success: false, message: `Couldn't rename "${systemName}": ${message}` },
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
    const message = error instanceof Error ? error.message : String(error);
    handleApplyResult(
      { success: false, message: `Couldn't delete "${label}": ${message}` },
      "",
    );
  }
}

function onContextMenuAction(action: "rename" | "delete") {
  const target = contextMenu.value;
  if (!target || target.kind !== "node") {
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

function onAddNodeAction(type: "output" | "input") {
  contextMenu.value = null;
  openNewDeviceDialog(type);
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

interface DropSlotPreview {
  groupId: string;
  axis: GroupLayoutAxis;
  edge: GroupEdge;
  /** Absolute canvas position, converted to group-relative when rendered. */
  position: { x: number; y: number };
  width: number;
  height: number;
}
const dropSlotPreview = ref<DropSlotPreview | null>(null);

const nodesWithLivePositions = computed<Node[]>(() => {
  if (draggingNodeIds.value.size === 0) {
    return built.value.nodes as Node[];
  }
  return (built.value.nodes as Node[]).map((node) => {
    if (!draggingNodeIds.value.has(node.id)) {
      return node;
    }
    const live = vueFlow.findNode(node.id);
    if (!live) return node;
    // A node with `parentNode` set is positioned relative to its parent, not
    // absolutely — substituting the live *absolute* computedPosition here
    // (as we do for top-level nodes, so an external graph-updated push mid-
    // drag can't snap them back) double-counts the group's own offset and
    // sends grouped members flying away from the cursor. Re-derive the
    // parent-relative position instead.
    if (node.parentNode) {
      const group = groups.value.find((entry) => entry.id === node.parentNode);
      if (group) {
        return {
          ...node,
          position: {
            x: live.computedPosition.x - group.position.x,
            y: live.computedPosition.y - group.position.y,
          },
        };
      }
    }
    return { ...node, position: live.computedPosition };
  });
});

// The drop-slot preview is deliberately NOT injected here as an extra vue-flow
// node: mutating the array bound to <VueFlow :nodes> on every drag tick (this
// fires continuously via @node-drag) makes Vue Flow re-reconcile its
// internally-tracked drag state against the "external" prop on every frame,
// which stomps on the dragged node's own live position — it stops tracking
// the cursor and snaps back to its pre-drag spot. Rendered as a plain
// transformed overlay instead (see dropSlotOverlayStyle), entirely outside
// vue-flow's controlled nodes array.
const nodes = computed<Node[]>(() => nodesWithLivePositions.value);

const dropSlotOverlayStyle = computed(() => {
  const preview = dropSlotPreview.value;
  if (!preview) return null;
  const viewport = vueFlow.viewport.value;
  return {
    left: `${preview.position.x * viewport.zoom + viewport.x}px`,
    top: `${preview.position.y * viewport.zoom + viewport.y}px`,
    width: `${preview.width * viewport.zoom}px`,
    height: `${preview.height * viewport.zoom}px`,
  };
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

// Announces the click-to-connect pickup step for keyboard/screen-reader users.
// Vue Flow's own click-connect state (`connectionClickStartHandle`) already
// drives the connect itself (see RoutingGraphNode.vue's Enter/Space handler,
// which just synthesizes a click on the focused port) — this only narrates
// the otherwise-silent "port picked up, waiting for a target" step; the
// eventual connect/disconnect outcome is already announced via the existing
// notices toast (`NoticeStack.vue`'s aria-live region).
const keyboardConnectMessage = ref("");
watch(vueFlow.connectionClickStartHandle, (handle) => {
  if (!handle) {
    keyboardConnectMessage.value = "";
    return;
  }
  const parsed = parseGraphNodeId(handle.nodeId);
  const label = parsed ? graphActions.labelForEntity(parsed.id) : handle.nodeId;
  // A connection can be picked up from either end — Vue Flow resolves which
  // side is the actual source/target once the second port is chosen.
  const direction = handle.type === "source" ? "output" : "input";
  const nextDirection = handle.type === "source" ? "an input" : "an output";
  keyboardConnectMessage.value = `Picked up ${label} ${direction} port. Tab to ${nextDirection} port and press Enter to connect, or press Escape to cancel.`;
});

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

/** Live position/size of every current member of `group`, from Vue Flow's own state. */
function groupMemberInputs(group: GraphGroup): GroupMemberInput[] {
  return group.memberIds
    .map((id) => {
      const memberNode = vueFlow.findNode(id);
      if (!memberNode) return null;
      return {
        id,
        position: { x: memberNode.computedPosition.x, y: memberNode.computedPosition.y },
        width: memberNode.dimensions.width || 200,
        height: memberNode.dimensions.height || 80,
      };
    })
    .filter((member): member is GroupMemberInput => member !== null);
}

/** Shrinks/grows a group's saved bounds to fit its current members' live positions. */
function resizeGroupToFitMembers(group: GraphGroup) {
  const members = groupMemberInputs(group);
  if (members.length === 0) return;
  const { position, size } = boundsForMembers(members);
  group.position = position;
  group.size = size;
}

/** Re-lays out `group`'s current members along `group.layoutAxis` and persists each new position. */
function reflowAndSaveGroup(group: GraphGroup) {
  if (!group.layoutAxis) {
    resizeGroupToFitMembers(group);
    return;
  }
  const members = groupMemberInputs(group);
  if (members.length === 0) return;
  const { positions, bounds } = reflowMembers(group.layoutAxis, members);
  for (const member of members) {
    const position = positions[member.id];
    saveNodePosition(member.id, position.x, position.y);
  }
  group.position = bounds.position;
  group.size = bounds.size;
}

/** Where a new member would land if inserted at `edge` of `group`, given its current members. */
function computeSlotPosition(
  group: GraphGroup,
  axis: GroupLayoutAxis,
  edge: GroupEdge,
  nodeRect: GraphRect,
): { x: number; y: number } {
  const members = groupMemberInputs(group);
  if (members.length === 0) {
    return { x: group.position.x, y: group.position.y };
  }
  if (axis === "row") {
    const top = Math.min(...members.map((member) => member.position.y));
    if (edge === "left") {
      const minX = Math.min(...members.map((member) => member.position.x));
      return { x: minX - MEMBER_GAP - nodeRect.width, y: top };
    }
    const maxX = Math.max(...members.map((member) => member.position.x + member.width));
    return { x: maxX + MEMBER_GAP, y: top };
  }
  const left = Math.min(...members.map((member) => member.position.x));
  if (edge === "top") {
    const minY = Math.min(...members.map((member) => member.position.y));
    return { x: left, y: minY - MEMBER_GAP - nodeRect.height };
  }
  const maxY = Math.max(...members.map((member) => member.position.y + member.height));
  return { x: left, y: maxY + MEMBER_GAP };
}

/** Finds which group (if any) a loose node dragged to `nodeRect` should join, and at which edge. */
function findDropTarget(
  nodeRect: GraphRect,
): { group: GraphGroup; axis: GroupLayoutAxis; edge: GroupEdge } | null {
  for (const group of groups.value) {
    const groupRect = {
      x: group.position.x,
      y: group.position.y,
      width: group.size.width,
      height: group.size.height,
    };
    const edge = nearestGroupEdge(nodeRect, groupRect);
    if (edge) {
      const axis: GroupLayoutAxis = edge === "left" || edge === "right" ? "row" : "column";
      return { group, axis, edge };
    }
  }
  return null;
}

function commitDirectionalInsert(
  group: GraphGroup,
  axis: GroupLayoutAxis,
  edge: GroupEdge,
  node: { id: string; dimensions: { width: number; height: number } },
) {
  const nodeRect = {
    x: 0,
    y: 0,
    width: node.dimensions.width || 200,
    height: node.dimensions.height || 80,
  };
  const slotPosition = computeSlotPosition(group, axis, edge, nodeRect);
  const newMember: GroupMemberInput = {
    id: node.id,
    position: slotPosition,
    width: nodeRect.width,
    height: nodeRect.height,
  };
  const existingMembers = groupMemberInputs(group);
  const prepend = edge === "left" || edge === "top";
  const orderedMembers = prepend ? [newMember, ...existingMembers] : [...existingMembers, newMember];

  const { positions, bounds } = reflowMembers(axis, orderedMembers);
  for (const member of orderedMembers) {
    const position = positions[member.id];
    saveNodePosition(member.id, position.x, position.y);
  }

  group.memberIds = orderedMembers.map((member) => member.id);
  group.layoutAxis = axis;
  group.position = bounds.position;
  group.size = bounds.size;
}

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
  dropSlotPreview.value = null;
}

function onNodeDrag(event: NodeDragEvent) {
  const node = event.node;
  // Directional-insert preview only applies to a single *loose* node (not
  // already in any group) being dragged toward a group — the detach flow
  // (onNodeDragStop) already owns the "member being pulled out" case, and a
  // parented member's `position` is parent-relative, not absolute, so it
  // can't be fed through the same absolute-rect math below.
  if (node.type === "groupNode" || node.type === "dropSlotNode" || node.parentNode) {
    dropSlotPreview.value = null;
    return;
  }

  // Unlike `computedPosition` (which vue-flow only resolves at drag-stop for
  // an un-parented node), `position` tracks the live cursor-driven location
  // throughout the drag — using computedPosition here left the preview
  // permanently anchored at the node's pre-drag spot.
  const nodeRect = {
    x: node.position.x,
    y: node.position.y,
    width: node.dimensions.width,
    height: node.dimensions.height,
  };
  const target = findDropTarget(nodeRect);
  if (!target) {
    dropSlotPreview.value = null;
    return;
  }

  const slotPosition = computeSlotPosition(target.group, target.axis, target.edge, nodeRect);
  dropSlotPreview.value = {
    groupId: target.group.id,
    axis: target.axis,
    edge: target.edge,
    position: slotPosition,
    width: nodeRect.width,
    height: nodeRect.height,
  };
}

function onNodeDragStop(event: NodeDragEvent) {
  const node = event.node;
  const idsToClear = event.nodes?.length ? event.nodes.map((n) => n.id) : [node.id];
  const next = new Set(draggingNodeIds.value);
  for (const id of idsToClear) {
    next.delete(id);
  }
  draggingNodeIds.value = next;
  dropSlotPreview.value = null;

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

  // Group membership check only considers the primary grabbed node — a node
  // landing inside/outside a group's bounds as a side effect of where other
  // multi-selected nodes were relative to the pointer shouldn't silently
  // join/detach it on its own.
  const nodeRect = {
    x: node.computedPosition.x,
    y: node.computedPosition.y,
    width: node.dimensions.width,
    height: node.dimensions.height,
  };

  const currentGroup = groups.value.find((entry) => entry.memberIds.includes(node.id));
  if (currentGroup) {
    const groupRect = {
      x: currentGroup.position.x,
      y: currentGroup.position.y,
      width: currentGroup.size.width,
      height: currentGroup.size.height,
    };
    if (containmentRatio(nodeRect, groupRect) < DETACH_THRESHOLD) {
      currentGroup.memberIds = currentGroup.memberIds.filter((id) => id !== node.id);
      if (currentGroup.memberIds.length === 0) {
        groups.value = groups.value.filter((entry) => entry.id !== currentGroup.id);
      } else {
        reflowAndSaveGroup(currentGroup);
      }
      persistGroups();
    }
    return;
  }

  // Node isn't in any group yet — a directional drop near an existing
  // group's edge (left/right/top/bottom) inserts it there, growing the
  // group and reflowing members into an aligned row/column.
  const target = findDropTarget(nodeRect);
  if (target) {
    commitDirectionalInsert(target.group, target.axis, target.edge, node);
    persistGroups();
  }
}

const MIN_GROUP_SELECTION = 2;

function isTypingTarget(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && ["INPUT", "TEXTAREA"].includes(target.tagName);
}

async function onWindowKeydown(event: KeyboardEvent) {
  if (event.key === "Escape" && vueFlow.connectionClickStartHandle.value) {
    event.preventDefault();
    vueFlow.connectionClickStartHandle.value = null;
    return;
  }

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
    <div class="routing-graph-live-region" aria-live="polite">{{ keyboardConnectMessage }}</div>
    <div class="routing-graph-legend" aria-label="Connection color legend">
      <span class="routing-graph-legend-title">Output connects to input</span>
      <div class="routing-graph-legend-items">
        <span v-for="entry in legend" :key="entry.key" class="routing-graph-legend-item">
          <span class="routing-graph-legend-swatch" :style="{ background: entry.color }" />
          {{ entry.label }}
        </span>
        <span class="routing-graph-legend-hint">
          Drag wire ends off a port to disconnect · Shift+drag to select multiple nodes · Press G to group ·
          Right-click empty space to add a node · Tab to a port and press Enter to connect it, Delete to
          disconnect it, Escape to cancel
        </span>
      </div>
    </div>
    <RoutingGraphContextMenu
      :target="contextMenu"
      @rename="onContextMenuAction('rename')"
      @delete="onContextMenuAction('delete')"
      @add-node="onAddNodeAction"
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
        @node-drag-start="onNodeDragStart"
        @node-drag="onNodeDrag"
        @node-drag-stop="onNodeDragStop"
        @pane-click="onPaneClick"
        @pane-context-menu="onPaneContextMenu"
      >
        <Background pattern-color="rgba(255,255,255,0.04)" :gap="20" />
        <Controls />
      </VueFlow>
      <div v-if="dropSlotOverlayStyle" class="routing-graph-drop-slot-overlay" :style="dropSlotOverlayStyle" />
    </div>
  </div>
</template>
