<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import NodeTypeIcon from "./NodeTypeIcon.vue";
import ToggleSwitch from "./ToggleSwitch.vue";
import { useApplyResult } from "../stores/notices";

const open = defineModel<boolean>({ required: true });

const { handleApplyResult } = useApplyResult();

const name = ref("");
const type = ref<"input" | "output">("output");
const multi = ref(false);
const nameInputRef = ref<HTMLInputElement | null>(null);

const canCreate = computed(() => name.value.trim().length > 0);

const slug = computed(() => {
  const trimmed = name.value.trim();
  if (!trimmed) return "";
  const dashed = trimmed
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return dashed ? `pipe-deck-${dashed}` : "";
});

function resetForm() {
  name.value = "";
  type.value = "output";
  multi.value = false;
}

function close() {
  open.value = false;
  resetForm();
}

watch(open, async (value) => {
  if (value) {
    await nextTick();
    nameInputRef.value?.focus();
  }
});

async function createVirtual() {
  const trimmed = name.value.trim();
  if (!trimmed) return;
  const command =
    type.value === "input"
      ? "create_virtual_input"
      : multi.value
        ? "create_virtual_multi_output"
        : "create_virtual_output";
  try {
    await invoke(command, { name: trimmed });
    handleApplyResult({ success: true }, `${trimmed} created`);
    close();
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}
</script>

<template>
  <div v-if="open" class="new-device-modal" @click.self="close">
    <div class="new-device-dialog" role="dialog" aria-modal="true" aria-labelledby="new-device-title">
      <h2 id="new-device-title">Create virtual device</h2>

      <div class="new-device-field">
        <label class="new-device-field-label" for="new-device-name">Name</label>
        <input
          id="new-device-name"
          ref="nameInputRef"
          v-model="name"
          type="text"
          placeholder="e.g. Game Mix"
          @keydown.enter="createVirtual"
        />
        <p v-if="slug" class="new-device-slug">
          <span class="new-device-slug-arrow">→</span> {{ slug }}
        </p>
      </div>

      <div class="new-device-field">
        <span class="new-device-field-label">Type</span>
        <div class="new-device-type-cards">
          <button
            type="button"
            class="new-device-type-card"
            :class="{ selected: type === 'output' }"
            @click="type = 'output'"
          >
            <NodeTypeIcon kind="virtual-sink" />
            <span class="new-device-type-card-title">Output</span>
            <span class="new-device-type-card-sub">Apps play into it, you route it onward</span>
          </button>
          <button
            type="button"
            class="new-device-type-card"
            :class="{ selected: type === 'input' }"
            @click="type = 'input'"
          >
            <NodeTypeIcon kind="virtual-input" />
            <span class="new-device-type-card-title">Input</span>
            <span class="new-device-type-card-sub">Mixes microphones into one virtual mic</span>
          </button>
        </div>
      </div>

      <div v-if="type === 'output'" class="new-device-toggle-row">
        <div class="new-device-toggle-copy">
          <span class="new-device-field-label">Multi-output</span>
          <p class="new-device-toggle-hint">Fan this sink out to several outputs at once, instead of just one.</p>
        </div>
        <ToggleSwitch v-model="multi" :show-state-labels="false" />
      </div>

      <div class="dialog-actions">
        <button type="button" @click="close">Cancel</button>
        <button type="button" class="primary" :disabled="!canCreate" @click="createVirtual">
          Create
        </button>
      </div>
    </div>
  </div>
</template>
