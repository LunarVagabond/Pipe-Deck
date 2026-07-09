<script setup lang="ts">
withDefaults(
  defineProps<{
    modelValue: boolean;
    disabled?: boolean;
    showStateLabels?: boolean;
  }>(),
  {
    disabled: false,
    showStateLabels: true,
  },
);

const emit = defineEmits<{
  "update:modelValue": [value: boolean];
}>();

function onChange(event: Event) {
  emit("update:modelValue", (event.target as HTMLInputElement).checked);
}
</script>

<template>
  <div class="toggle-switch-row" :class="{ 'toggle-switch-row--compact': !showStateLabels }">
    <span
      v-if="showStateLabels"
      class="toggle-state-label"
      :class="{ active: !modelValue }"
    >
      Off
    </span>
    <label class="toggle-switch">
      <input
        type="checkbox"
        class="toggle-input"
        :checked="modelValue"
        :disabled="disabled"
        @change="onChange"
      />
      <span class="toggle-track" aria-hidden="true">
        <span class="toggle-thumb" />
      </span>
    </label>
    <span
      v-if="showStateLabels"
      class="toggle-state-label"
      :class="{ active: modelValue }"
    >
      On
    </span>
  </div>
</template>
