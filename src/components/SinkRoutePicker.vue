<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useApplyResult } from "../stores/notices";
import type { Device } from "../types/graph";
import { targetLabel } from "../utils/routingLayout";

const { sink, targets } = defineProps<{
  sink: Device;
  targets: Device[];
}>();

const { handleApplyResult } = useApplyResult();

const rootRef = ref<HTMLElement | null>(null);
const open = ref(false);
const pendingIds = ref<string[]>([]);

const selectedIds = computed(() => {
  if (sink.current_targets?.length) {
    return sink.current_targets;
  }
  return sink.current_target ? [sink.current_target] : [];
});

const summaryLabel = computed(() => {
  const ids = selectedIds.value;
  if (ids.length === 0) {
    return "Select outputs";
  }

  const labels = ids
    .map((id) => targets.find((target) => target.id === id))
    .filter((target): target is Device => Boolean(target))
    .map((target) => targetLabel(target));

  if (labels.length === 1) {
    return labels[0];
  }
  if (labels.length === 2) {
    return `${labels[0]}, ${labels[1]}`;
  }
  return `${labels[0]} + ${labels.length - 1} more`;
});

watch(
  selectedIds,
  (ids) => {
    pendingIds.value = [...ids];
  },
  { immediate: true },
);

function toggleOpen() {
  open.value = !open.value;
  if (open.value) {
    pendingIds.value = [...selectedIds.value];
  }
}

function onDocumentPointerDown(event: PointerEvent) {
  if (!open.value || !rootRef.value) return;
  if (!rootRef.value.contains(event.target as Node)) {
    open.value = false;
  }
}

onMounted(() => {
  document.addEventListener("pointerdown", onDocumentPointerDown);
});

onUnmounted(() => {
  document.removeEventListener("pointerdown", onDocumentPointerDown);
});

async function applyTargets(targetDeviceIds: string[]) {
  try {
    const result = await invoke<{ success: boolean; message?: string }>("set_device_targets", {
      sourceDeviceId: sink.id,
      targetDeviceIds,
    });
    handleApplyResult(result, "Sink routing updated");
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

function isChecked(targetId: string) {
  return pendingIds.value.includes(targetId);
}

async function onCheckboxChange(targetId: string, event: Event) {
  const checked = (event.target as HTMLInputElement).checked;
  const next = checked
    ? [...pendingIds.value, targetId]
    : pendingIds.value.filter((id) => id !== targetId);

  if (next.length === 0) {
    pendingIds.value = next;
    await applyTargets(next);
    return;
  }

  pendingIds.value = next;
  await applyTargets(next);
}
</script>

<template>
  <div ref="rootRef" class="sink-route-picker playback">
    <div class="route-dropdown">
      <button
        type="button"
        class="routing-select route-dropdown-trigger"
        :aria-expanded="open"
        @click="toggleOpen"
      >
        <span class="route-dropdown-label">{{ summaryLabel }}</span>
      </button>

      <div v-if="open" class="route-dropdown-panel" role="listbox" :aria-label="`Route ${sink.label}`">
        <label
          v-for="target in targets"
          :key="target.id"
          class="route-dropdown-option"
        >
          <input
            type="checkbox"
            :checked="isChecked(target.id)"
            @change="onCheckboxChange(target.id, $event)"
          />
          <span>{{ targetLabel(target) }}</span>
        </label>
      </div>
    </div>
  </div>
</template>
