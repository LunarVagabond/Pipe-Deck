<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import NodeTypeIcon from "./NodeTypeIcon.vue";
import { useApplyResult } from "../stores/notices";
import { useNewDeviceDialog } from "../stores/newDeviceDialog";

const { handleApplyResult } = useApplyResult();
const { newDeviceDialogState, closeNewDeviceDialog } = useNewDeviceDialog();

const open = computed(() => newDeviceDialogState.value.open);
const name = ref("");
const type = ref<"input" | "bus" | "output">("bus");
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
  type.value = "bus";
}

function close() {
  closeNewDeviceDialog();
  resetForm();
}

watch(open, async (value) => {
  if (value) {
    type.value = newDeviceDialogState.value.type;
    await nextTick();
    nameInputRef.value?.focus();
  }
});

async function createVirtual() {
  const trimmed = name.value.trim();
  if (!trimmed) return;
  const command = type.value === "input" ? "create_virtual_input" : "create_virtual_output";
  const args = type.value === "input" ? { name: trimmed } : { name: trimmed, role: type.value };
  try {
    await invoke(command, args);
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
            :class="{ selected: type === 'bus' }"
            @click="type = 'bus'"
          >
            <NodeTypeIcon kind="virtual-sink" />
            <span class="new-device-type-card-title">Bus</span>
            <span class="new-device-type-card-sub">Apps play into it, effects can attach, and you route it onward — even into another bus, to build a submix</span>
          </button>
          <button
            type="button"
            class="new-device-type-card"
            :class="{ selected: type === 'output' }"
            @click="type = 'output'"
          >
            <NodeTypeIcon kind="virtual-output" />
            <span class="new-device-type-card-title">Output (virtual)</span>
            <span class="new-device-type-card-sub">Apps play into it, but it's a destination — no effects, and it can't be routed onward</span>
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

      <div class="dialog-actions">
        <button type="button" @click="close">Cancel</button>
        <button type="button" class="primary" :disabled="!canCreate" @click="createVirtual">
          Create
        </button>
      </div>
    </div>
  </div>
</template>
