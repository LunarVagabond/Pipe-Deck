<script setup lang="ts">
import { nextTick, ref, watch } from "vue";

const props = defineProps<{
  label: string;
  editable?: boolean;
  deletable?: boolean;
  layout?: "inline" | "stacked";
  showLabelTooltip?: boolean;
}>();

const emit = defineEmits<{
  save: [name: string];
  delete: [];
}>();

const editing = ref(false);
const draft = ref(props.label);
const inputRef = ref<HTMLInputElement | null>(null);

watch(
  () => props.label,
  (value) => {
    if (!editing.value) {
      draft.value = value;
    }
  },
);

async function startEdit() {
  if (!props.editable) return;
  editing.value = true;
  draft.value = props.label;
  await nextTick();
  inputRef.value?.focus();
  inputRef.value?.select();
}

function cancelEdit() {
  editing.value = false;
  draft.value = props.label;
}

function commitEdit() {
  const next = draft.value.trim();
  editing.value = false;
  if (next && next !== props.label) {
    emit("save", next);
  }
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Enter") {
    event.preventDefault();
    commitEdit();
  }
  if (event.key === "Escape") {
    cancelEdit();
  }
}
</script>

<template>
  <div class="node-card-header" :class="{ 'is-stacked': layout === 'stacked' }">
    <template v-if="layout === 'stacked'">
      <div
        v-if="editable || deletable || $slots['toolbar-extra']"
        class="node-card-toolbar"
      >
        <div v-if="editable || deletable" class="node-card-actions">
          <button
            v-if="editable"
            type="button"
            class="icon-btn edit-btn"
            aria-label="Rename"
            @click="startEdit"
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path
                d="M4 20h4l10.5-10.5a1.5 1.5 0 0 0 0-2.12L15.62 4.5a1.5 1.5 0 0 0-2.12 0L3 15v5z"
                fill="none"
                stroke="currentColor"
                stroke-width="1.75"
                stroke-linejoin="round"
              />
            </svg>
          </button>
          <button
            v-if="deletable"
            type="button"
            class="icon-btn delete-btn"
            aria-label="Delete"
            @click="emit('delete')"
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path
                d="M4 7h16M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2m-1 12H10a2 2 0 0 1-2-2V7h10v10a2 2 0 0 1-2 2z"
                fill="none"
                stroke="currentColor"
                stroke-width="1.75"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          </button>
        </div>
        <slot name="toolbar-extra" />
      </div>
      <div class="node-card-title">
        <input
          v-if="editing && editable"
          ref="inputRef"
          v-model="draft"
          class="node-card-title-input"
          @blur="commitEdit"
          @keydown="onKeydown"
        />
        <strong v-else :title="showLabelTooltip ? label : undefined">{{ label }}</strong>
      </div>
    </template>
    <template v-else>
      <div class="node-card-title">
        <input
          v-if="editing && editable"
          ref="inputRef"
          v-model="draft"
          class="node-card-title-input"
          @blur="commitEdit"
          @keydown="onKeydown"
        />
        <strong v-else :title="showLabelTooltip ? label : undefined">{{ label }}</strong>
      </div>
      <div v-if="editable || deletable" class="node-card-actions">
        <button
          v-if="editable"
          type="button"
          class="icon-btn edit-btn"
          aria-label="Rename"
          @click="startEdit"
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path
              d="M4 20h4l10.5-10.5a1.5 1.5 0 0 0 0-2.12L15.62 4.5a1.5 1.5 0 0 0-2.12 0L3 15v5z"
              fill="none"
              stroke="currentColor"
              stroke-width="1.75"
              stroke-linejoin="round"
            />
          </svg>
        </button>
        <button
          v-if="deletable"
          type="button"
          class="icon-btn delete-btn"
          aria-label="Delete"
          @click="emit('delete')"
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path
              d="M4 7h16M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2m-1 12H10a2 2 0 0 1-2-2V7h10v10a2 2 0 0 1-2 2z"
              fill="none"
              stroke="currentColor"
              stroke-width="1.75"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </button>
      </div>
    </template>
  </div>
</template>
