<script setup lang="ts">
withDefaults(
  defineProps<{
    modelValue: string;
    options: { value: string; label: string }[];
    disabled?: boolean;
  }>(),
  {
    disabled: false,
  },
);

const emit = defineEmits<{
  "update:modelValue": [value: string];
}>();
</script>

<template>
  <div class="segmented-control" role="radiogroup">
    <button
      v-for="option in options"
      :key="option.value"
      type="button"
      role="radio"
      class="segmented-control-option"
      :class="{ active: modelValue === option.value }"
      :aria-checked="modelValue === option.value"
      :disabled="disabled"
      @click="emit('update:modelValue', option.value)"
    >
      {{ option.label }}
    </button>
  </div>
</template>
