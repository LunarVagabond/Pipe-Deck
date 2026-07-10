<script setup lang="ts">
import { computed, inject, onUnmounted, ref, type Ref } from "vue";
import { BaseEdge, type EdgeProps, getSmoothStepPath, useVueFlow } from "@vue-flow/core";
import { routingGraphActionsKey } from "../composables/routingGraphContext";
import {
  addReroute,
  getReroutes,
  removeReroute,
  rerouteEdgeKey,
  rerouteRevision,
  updateReroute,
  type RerouteKnot,
} from "./routing-graph/rerouteLayout";

const props = defineProps<EdgeProps>();

const { screenToFlowCoordinate } = useVueFlow();
const actions = inject(routingGraphActionsKey, null);
const selectedReroute = inject<Ref<{ edgeKey: string; knotId: string } | null> | null>(
  "routing-selected-reroute",
  null,
);

const edgeKey = computed(() => rerouteEdgeKey(props.source, props.target));

const dragPositions = ref<Record<string, { x: number; y: number }>>({});
let activeDrag: { knotId: string; pointerId: number } | null = null;

const storedWaypoints = computed(() => {
  rerouteRevision.value;
  return getReroutes(edgeKey.value);
});

const waypoints = computed(() =>
  storedWaypoints.value.map((knot) => {
    const drag = dragPositions.value[knot.id];
    return drag ? { ...knot, x: drag.x, y: drag.y } : knot;
  }),
);

const hasReroutes = computed(() => storedWaypoints.value.length > 0);

const points = computed(() => [
  { x: props.sourceX, y: props.sourceY },
  ...waypoints.value.map((knot) => ({ x: knot.x, y: knot.y })),
  { x: props.targetX, y: props.targetY },
]);

function polylinePath(pts: Array<{ x: number; y: number }>): string {
  if (pts.length < 2) {
    return "";
  }
  return pts
    .map((point, index) => `${index === 0 ? "M" : "L"} ${point.x} ${point.y}`)
    .join(" ");
}

const wirePath = computed(() => (hasReroutes.value ? polylinePath(points.value) : ""));

const directPath = computed(() => {
  if (hasReroutes.value) {
    return "";
  }
  const [path] = getSmoothStepPath({
    sourceX: props.sourceX,
    sourceY: props.sourceY,
    targetX: props.targetX,
    targetY: props.targetY,
    sourcePosition: props.sourcePosition,
    targetPosition: props.targetPosition,
  });
  return path;
});

const arrowPath = computed(() => {
  const pts = points.value;
  if (pts.length < 2) {
    return "";
  }
  const from = pts[pts.length - 2];
  const to = pts[pts.length - 1];
  return `M ${from.x} ${from.y} L ${to.x} ${to.y}`;
});

const hitPath = computed(() => (hasReroutes.value ? wirePath.value : directPath.value));

function flowPointFromEvent(event: MouseEvent | PointerEvent) {
  return screenToFlowCoordinate({ x: event.clientX, y: event.clientY });
}

function onPathDoubleClick(event: MouseEvent) {
  const point = flowPointFromEvent(event);
  addReroute(edgeKey.value, point.x, point.y);
}

function onPathContextMenu(event: MouseEvent) {
  event.preventDefault();
  event.stopPropagation();
  actions?.openMenu({
    kind: "edge",
    x: event.clientX,
    y: event.clientY,
    edgeId: edgeKey.value,
    hasReroutes: hasReroutes.value,
  });
}

function finishDrag(knotId: string, x: number, y: number) {
  updateReroute(edgeKey.value, knotId, x, y);
  const next = { ...dragPositions.value };
  delete next[knotId];
  dragPositions.value = next;
  activeDrag = null;
  window.removeEventListener("pointermove", onWindowPointerMove);
  window.removeEventListener("pointerup", onWindowPointerUp);
  window.removeEventListener("pointercancel", onWindowPointerUp);
}

function onWindowPointerMove(event: PointerEvent) {
  if (!activeDrag) {
    return;
  }
  event.preventDefault();
  const point = flowPointFromEvent(event);
  dragPositions.value = {
    ...dragPositions.value,
    [activeDrag.knotId]: { x: point.x, y: point.y },
  };
}

function onWindowPointerUp(event: PointerEvent) {
  if (!activeDrag) {
    return;
  }
  const point = dragPositions.value[activeDrag.knotId] ?? flowPointFromEvent(event);
  finishDrag(activeDrag.knotId, point.x, point.y);
}

function onKnotPointerDown(knot: RerouteKnot, event: PointerEvent) {
  if (event.altKey) {
    removeReroute(edgeKey.value, knot.id);
    if (selectedReroute?.value?.knotId === knot.id) {
      selectedReroute.value = null;
    }
    return;
  }

  event.stopPropagation();
  event.preventDefault();

  activeDrag = { knotId: knot.id, pointerId: event.pointerId };

  if (selectedReroute) {
    selectedReroute.value = { edgeKey: edgeKey.value, knotId: knot.id };
  }

  const point = flowPointFromEvent(event);
  dragPositions.value = {
    ...dragPositions.value,
    [knot.id]: { x: point.x, y: point.y },
  };

  window.addEventListener("pointermove", onWindowPointerMove);
  window.addEventListener("pointerup", onWindowPointerUp);
  window.addEventListener("pointercancel", onWindowPointerUp);
}

function onKnotClick(knot: RerouteKnot, event: MouseEvent) {
  event.stopPropagation();
  if (selectedReroute) {
    selectedReroute.value = { edgeKey: edgeKey.value, knotId: knot.id };
  }
}

onUnmounted(() => {
  window.removeEventListener("pointermove", onWindowPointerMove);
  window.removeEventListener("pointerup", onWindowPointerUp);
  window.removeEventListener("pointercancel", onWindowPointerUp);
  activeDrag = null;
  dragPositions.value = {};
});
</script>

<template>
  <g class="routing-graph-edge">
    <template v-if="hasReroutes">
      <BaseEdge
        :id="id"
        :path="wirePath"
        :style="style"
        :interaction-width="interactionWidth ?? 22"
      />
      <BaseEdge
        :id="`${id}-arrow`"
        :path="arrowPath"
        :style="style"
        :marker-end="markerEnd"
        :interaction-width="0"
      />
    </template>
    <BaseEdge
      v-else
      :id="id"
      :path="directPath"
      :style="style"
      :marker-end="markerEnd"
      :interaction-width="interactionWidth ?? 22"
    />
    <path
      :d="hitPath"
      fill="none"
      stroke="transparent"
      stroke-width="16"
      class="routing-graph-edge-hit nopan"
      @dblclick="onPathDoubleClick"
      @contextmenu="onPathContextMenu"
    />
    <g
      v-for="knot in waypoints"
      :key="knot.id"
      class="routing-reroute-knot-wrap nopan"
      @pointerdown="onKnotPointerDown(knot, $event)"
      @click="onKnotClick(knot, $event)"
    >
      <circle
        class="routing-reroute-knot-hit"
        :cx="knot.x"
        :cy="knot.y"
        r="12"
        fill="transparent"
      />
      <circle
        class="routing-reroute-knot"
        :class="{
          selected: selectedReroute?.edgeKey === edgeKey && selectedReroute?.knotId === knot.id,
        }"
        :cx="knot.x"
        :cy="knot.y"
        r="5"
      />
    </g>
  </g>
</template>
