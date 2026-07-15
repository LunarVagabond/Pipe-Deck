<script setup lang="ts">
import { inject, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import type { RoutingGraphGroupData } from "./routing-graph/buildGraph";
import { GROUP_COLORS } from "./routing-graph/groups";
import { routingGraphActionsKey } from "../composables/routingGraphContext";

const props = defineProps<{
  data: RoutingGraphGroupData;
}>();

const actions = inject(routingGraphActionsKey, null);

const editing = ref(false);
const draft = ref(props.data.label);
const inputRef = ref<HTMLInputElement | null>(null);
const colorPickerOpen = ref(false);
const colorChoices = GROUP_COLORS;
const colorPickerRef = ref<HTMLElement | null>(null);

function pickColor(color: string) {
  actions?.setGroupColor(props.data.groupId, color);
  colorPickerOpen.value = false;
}

function onDocumentPointerDown(event: PointerEvent) {
  if (!colorPickerOpen.value) return;
  if (event.target instanceof Node && colorPickerRef.value?.contains(event.target)) return;
  colorPickerOpen.value = false;
}

onMounted(() => document.addEventListener("pointerdown", onDocumentPointerDown));
onUnmounted(() => document.removeEventListener("pointerdown", onDocumentPointerDown));

watch(
  () => props.data.label,
  (value) => {
    if (!editing.value) draft.value = value;
  },
);

async function startEdit() {
  editing.value = true;
  draft.value = props.data.label;
  await nextTick();
  inputRef.value?.focus();
  inputRef.value?.select();
}

function commitEdit() {
  const next = draft.value.trim();
  editing.value = false;
  if (next && next !== props.data.label) {
    actions?.renameGroup(props.data.groupId, next);
  }
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Enter") {
    event.preventDefault();
    commitEdit();
  }
  if (event.key === "Escape") {
    editing.value = false;
    draft.value = props.data.label;
  }
}
</script>

<template>
  <div class="routing-graph-group" :style="data.color ? { '--group-color': data.color } : undefined">
    <div class="routing-graph-group-header group-drag-handle" @dblclick="startEdit">
      <div class="routing-graph-group-color" ref="colorPickerRef">
        <button
          type="button"
          class="routing-graph-group-color-swatch"
          title="Group color"
          aria-label="Group color"
          :style="{ backgroundColor: data.color || 'transparent' }"
          @pointerdown.stop
          @click.stop="colorPickerOpen = !colorPickerOpen"
        />
        <div v-if="colorPickerOpen" class="routing-graph-group-color-popover" @pointerdown.stop>
          <button
            v-for="color in colorChoices"
            :key="color"
            type="button"
            class="routing-graph-group-color-option"
            :style="{ backgroundColor: color }"
            :aria-label="`Set group color to ${color}`"
            @click.stop="pickColor(color)"
          />
        </div>
      </div>
      <input
        v-if="editing"
        ref="inputRef"
        v-model="draft"
        class="routing-graph-group-title-input"
        @blur="commitEdit"
        @keydown="onKeydown"
        @pointerdown.stop
      />
      <span v-else class="routing-graph-group-title">{{ data.label }}</span>
      <button
        type="button"
        class="routing-graph-group-ungroup"
        title="Ungroup"
        aria-label="Ungroup"
        @pointerdown.stop
        @click.stop="actions?.ungroup(data.groupId)"
      >
        ×
      </button>
    </div>
    <div class="routing-graph-group-body" />
  </div>
</template>
