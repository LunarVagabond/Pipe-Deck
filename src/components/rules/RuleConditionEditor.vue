<script setup lang="ts">
import type { RuleCondition } from "../../types/graph";
import {
  CATEGORY_OPTIONS,
  CONDITION_TYPE_OPTIONS,
  DIRECTION_OPTIONS,
  REGEX_FIELD_OPTIONS,
  conditionTypeMeta,
  conditionValue,
  setConditionType,
  setConditionValue,
  type ConditionType,
} from "../../utils/ruleConditions";

const { condition, active, canRemove, suggestions } = defineProps<{
  condition: RuleCondition;
  active: boolean;
  canRemove: boolean;
  suggestions: string[];
}>();

const emit = defineEmits<{
  activate: [];
  remove: [];
}>();
</script>

<template>
  <div class="condition-card" :class="{ active }" @click="emit('activate')">
    <div class="condition-row">
      <label class="condition-field">
        <span class="field-label">Match by</span>
        <select
          :value="condition.type"
          @change="
            setConditionType(condition, ($event.target as HTMLSelectElement).value as ConditionType)
          "
        >
          <option v-for="option in CONDITION_TYPE_OPTIONS" :key="option.type" :value="option.type">
            {{ option.label }}
          </option>
        </select>
      </label>

      <template v-if="condition.type === 'regex'">
        <label class="condition-field">
          <span class="field-label">Field</span>
          <select v-model="condition.field">
            <option v-for="option in REGEX_FIELD_OPTIONS" :key="option.value" :value="option.value">
              {{ option.label }}
            </option>
          </select>
        </label>
        <label class="condition-field condition-field-grow">
          <span class="field-label">Pattern</span>
          <input v-model="condition.pattern" type="text" placeholder="e.g. Discord.*" />
        </label>
      </template>

      <template v-else-if="condition.type === 'direction'">
        <label class="condition-field condition-field-grow">
          <span class="field-label">Value</span>
          <select
            :value="conditionValue(condition)"
            @change="setConditionValue(condition, ($event.target as HTMLSelectElement).value)"
          >
            <option value="" disabled>Select direction</option>
            <option v-for="option in DIRECTION_OPTIONS" :key="option.value" :value="option.value">
              {{ option.label }}
            </option>
          </select>
        </label>
      </template>

      <template v-else-if="condition.type === 'category'">
        <label class="condition-field condition-field-grow">
          <span class="field-label">Value</span>
          <select
            :value="conditionValue(condition)"
            @change="setConditionValue(condition, ($event.target as HTMLSelectElement).value)"
          >
            <option value="" disabled>Select category</option>
            <option v-for="category in CATEGORY_OPTIONS" :key="category" :value="category">
              {{ category }}
            </option>
          </select>
        </label>
      </template>

      <template v-else>
        <label class="condition-field condition-field-grow">
          <span class="field-label">Value</span>
          <input
            :value="conditionValue(condition)"
            type="text"
            :placeholder="conditionTypeMeta(condition.type).placeholder"
            @input="setConditionValue(condition, ($event.target as HTMLInputElement).value)"
          />
        </label>
      </template>

      <button
        type="button"
        class="condition-remove"
        :disabled="!canRemove"
        @click.stop="emit('remove')"
      >
        Remove
      </button>
    </div>

    <p class="condition-help">
      {{ conditionTypeMeta(condition.type).description }}
      <span class="condition-example">Example: {{ conditionTypeMeta(condition.type).example }}</span>
    </p>

    <div v-if="suggestions.length" class="condition-suggestions">
      <span class="condition-suggestions-label">From active audio:</span>
      <button
        v-for="value in suggestions"
        :key="`${condition.type}-${value}`"
        type="button"
        class="condition-suggestion-chip"
        @click="setConditionValue(condition, value)"
      >
        {{ value }}
      </button>
    </div>
  </div>
</template>
