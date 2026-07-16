<script setup lang="ts">
import { ref } from "vue";
import { useEffectChain } from "../composables/useEffectChain";
import { emptyEq5BandStage } from "../types/graph";
import type { EffectStage, Eq5BandStage } from "../types/graph";

const props = withDefaults(
  defineProps<{
    deviceId: string;
    /** Tight layout for the Routing graph node — no "+ Add effect" button
     * (the node's right-click menu covers adding), sliders start collapsed. */
    compact?: boolean;
  }>(),
  { compact: false },
);

const { chainFor, removeStage, reorderStages, scheduleStageUpdate, addEq5BandStage } = useEffectChain();

const expandedStageIds = ref<Set<string>>(new Set());
const draggedStageId = ref<string | null>(null);

const eqBands: { key: keyof Omit<Eq5BandStage, "kind" | "id">; label: string; hint: string }[] = [
  { key: "eq_sub", label: "Sub", hint: "60 Hz" },
  { key: "eq_bass", label: "Bass", hint: "150 Hz" },
  { key: "eq_mid", label: "Mid", hint: "1 kHz" },
  { key: "eq_treble", label: "Treble", hint: "4 kHz" },
  { key: "eq_air", label: "Air", hint: "10 kHz" },
  { key: "output_gain", label: "Gain", hint: "trim" },
];

function toggleExpanded(stageId: string) {
  const next = new Set(expandedStageIds.value);
  if (next.has(stageId)) {
    next.delete(stageId);
  } else {
    next.add(stageId);
  }
  expandedStageIds.value = next;
}

function stageLabel(stage: Eq5BandStage): string {
  return stage.kind === "eq5band" ? "5-Band EQ" : stage.kind;
}

function onSliderInput(stage: Eq5BandStage, key: keyof Omit<Eq5BandStage, "kind" | "id">, event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  scheduleStageUpdate(props.deviceId, { ...stage, [key]: value });
}

/** Resets one stage's parameters back to their defaults in place — same
 * stage, same position in the chain, just neutral values. Keyed by kind so
 * a future second `EffectStage` variant is a one-line addition here rather
 * than a rewrite. */
function resetStage(stage: EffectStage) {
  if (stage.kind === "eq5band") {
    scheduleStageUpdate(props.deviceId, emptyEq5BandStage(stage.id));
  }
}

function hasEq5Band(): boolean {
  return chainFor(props.deviceId).stages.some((stage) => stage.kind === "eq5band");
}

function onDragStart(stageId: string) {
  draggedStageId.value = stageId;
}

function onDrop(targetStageId: string) {
  const dragged = draggedStageId.value;
  draggedStageId.value = null;
  if (!dragged || dragged === targetStageId) return;

  const ids = chainFor(props.deviceId).stages.map((stage) => stage.id);
  const fromIndex = ids.indexOf(dragged);
  const toIndex = ids.indexOf(targetStageId);
  if (fromIndex === -1 || toIndex === -1) return;

  ids.splice(fromIndex, 1);
  ids.splice(toIndex, 0, dragged);
  void reorderStages(props.deviceId, ids);
}
</script>

<template>
  <div class="effect-stage-list" :class="{ 'effect-stage-list--compact': compact }">
    <div
      v-for="stage in chainFor(deviceId).stages"
      :key="stage.id"
      class="effect-stage"
      :class="{ dragging: draggedStageId === stage.id }"
      @dragover.prevent
      @drop="onDrop(stage.id)"
    >
      <div class="effect-stage-header">
        <span
          class="effect-stage-drag-handle"
          aria-hidden="true"
          draggable="true"
          @dragstart="onDragStart(stage.id)"
          >⠿</span
        >
        <button
          type="button"
          class="effect-stage-name"
          :aria-expanded="expandedStageIds.has(stage.id)"
          @click="toggleExpanded(stage.id)"
        >
          {{ stageLabel(stage) }}
        </button>
        <button
          type="button"
          class="effect-stage-remove"
          aria-label="Remove effect"
          @click="removeStage(deviceId, stage.id)"
        >
          ×
        </button>
      </div>

      <div v-if="expandedStageIds.has(stage.id) && stage.kind === 'eq5band'" class="effect-stage-sliders">
        <label v-for="band in eqBands" :key="band.key" class="effect-stage-slider-row">
          <span class="effect-stage-slider-label">
            {{ band.label }}
            <em>{{ band.hint }}</em>
          </span>
          <input
            type="range"
            min="-12"
            max="12"
            step="1"
            :value="stage[band.key]"
            @input="onSliderInput(stage, band.key, $event)"
          />
          <span class="effect-stage-slider-value">{{ stage[band.key] }}</span>
        </label>
        <button type="button" class="effect-stage-reset" @click="resetStage(stage)">
          Reset to default
        </button>
      </div>
    </div>

    <button
      v-if="!compact && !hasEq5Band()"
      type="button"
      class="effect-stage-add"
      @click="addEq5BandStage(deviceId)"
    >
      + Add effect
    </button>
  </div>
</template>
