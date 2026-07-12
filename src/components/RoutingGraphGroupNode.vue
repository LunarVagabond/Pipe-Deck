<script setup lang="ts">
import { inject, nextTick, ref, watch } from "vue";
import type { RoutingGraphGroupData } from "./routing-graph/buildGraph";
import { routingGraphActionsKey } from "../composables/routingGraphContext";

const props = defineProps<{
  data: RoutingGraphGroupData;
}>();

const actions = inject(routingGraphActionsKey, null);

const editing = ref(false);
const draft = ref(props.data.label);
const inputRef = ref<HTMLInputElement | null>(null);

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
  <div class="routing-graph-group">
    <div class="routing-graph-group-header group-drag-handle" @dblclick="startEdit">
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
