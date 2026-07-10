<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { usePrompt } from "../stores/prompt";

const { promptState, answer, updateValue } = usePrompt();
const inputRef = ref<HTMLInputElement | null>(null);

watch(
  () => promptState.value.open,
  async (open) => {
    if (!open) {
      return;
    }
    await nextTick();
    inputRef.value?.focus();
    inputRef.value?.select();
  },
);

function onSubmit() {
  const trimmed = promptState.value.value.trim();
  answer(trimmed || null);
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Enter") {
    event.preventDefault();
    onSubmit();
  }
  if (event.key === "Escape") {
    answer(null);
  }
}
</script>

<template>
  <div
    v-if="promptState.open"
    class="prompt-dialog-overlay"
    @click.self="answer(null)"
  >
    <form
      class="prompt-dialog"
      role="dialog"
      aria-modal="true"
      @submit.prevent="onSubmit"
    >
      <h2>{{ promptState.title }}</h2>
      <p v-if="promptState.message">{{ promptState.message }}</p>
      <input
        ref="inputRef"
        :value="promptState.value"
        type="text"
        class="prompt-dialog-input"
        :placeholder="promptState.placeholder"
        @input="updateValue(($event.target as HTMLInputElement).value)"
        @keydown="onKeydown"
      />
      <div class="prompt-dialog-actions">
        <button type="button" @click="answer(null)">
          {{ promptState.cancelLabel }}
        </button>
        <button type="submit" class="primary">
          {{ promptState.confirmLabel }}
        </button>
      </div>
    </form>
  </div>
</template>
