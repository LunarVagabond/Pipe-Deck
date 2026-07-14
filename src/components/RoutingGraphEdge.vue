<script setup lang="ts">
import { computed } from "vue";
import { BaseEdge, EdgeLabelRenderer, getBezierPath, getSmoothStepPath } from "@vue-flow/core";
import type { EdgeProps } from "@vue-flow/core";
import { useConnectionEffects } from "../composables/useConnectionEffects";
import type { ConnectionEffectKind } from "../types/graph";

interface RoutingGraphEdgeData {
  effects?: ConnectionEffectKind[];
  rawSourceId: string;
  rawTargetId: string;
  isBackward?: boolean;
}

const props = defineProps<EdgeProps<RoutingGraphEdgeData>>();

const { pendingVolumes, scheduleConnectionVolume } = useConnectionEffects();

// Backward-flowing connections are routed as an orthogonal smoothstep (see
// collectEdges.ts's `isBackward` comment); everything else uses the default
// bezier. Match that here so the path looks identical to before this
// component existed (which used vue-flow's built-in `type: "smoothstep"`
// edge selection instead of this data flag).
const pathParams = computed(() => {
  const options = {
    sourceX: props.sourceX,
    sourceY: props.sourceY,
    sourcePosition: props.sourcePosition,
    targetX: props.targetX,
    targetY: props.targetY,
    targetPosition: props.targetPosition,
  };
  return props.data?.isBackward ? getSmoothStepPath(options) : getBezierPath(options);
});

const volumeEffect = computed(() =>
  props.data?.effects?.find((effect): effect is Extract<ConnectionEffectKind, { kind: "volume" }> => effect.kind === "volume"),
);

const displayVolume = computed(
  () => pendingVolumes.value[props.id] ?? volumeEffect.value?.volume_percent ?? 100,
);

function onVolumeInput(event: Event) {
  const percent = Number((event.target as HTMLInputElement).value);
  scheduleConnectionVolume(props.id, props.data.rawSourceId, props.data.rawTargetId, percent);
}
</script>

<template>
  <BaseEdge
    :id="id"
    :path="pathParams[0]"
    :marker-end="markerEnd"
    :style="style"
    :interaction-width="interactionWidth"
  />
  <EdgeLabelRenderer v-if="volumeEffect">
    <div
      class="routing-graph-edge-volume nodrag nopan"
      :style="{
        position: 'absolute',
        transform: `translate(-50%, -50%) translate(${pathParams[1]}px, ${pathParams[2]}px)`,
      }"
    >
      <input
        type="range"
        min="0"
        max="100"
        :value="displayVolume"
        :aria-label="'Connection volume'"
        @input="onVolumeInput"
      />
      <span>{{ displayVolume }}%</span>
    </div>
  </EdgeLabelRenderer>
</template>
