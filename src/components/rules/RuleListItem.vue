<script setup lang="ts">
import ToggleSwitch from "../ToggleSwitch.vue";
import type { Rule } from "../../types/graph";
import { conditionTypeLabel, formatConditionSummary } from "../../utils/ruleConditions";

const { rule, targetKindLabel, targetName, liveMatchCount, canMoveUp, canMoveDown } = defineProps<{
  rule: Rule;
  targetKindLabel: string;
  targetName?: string;
  liveMatchCount: number;
  canMoveUp: boolean;
  canMoveDown: boolean;
}>();

const emit = defineEmits<{
  edit: [];
  delete: [];
  "toggle-enabled": [enabled: boolean];
  "move-up": [];
  "move-down": [];
}>();
</script>

<template>
  <tr class="rule-row" :class="{ 'rule-row-disabled': !rule.enabled }">
    <td class="rules-rule-cell">
      <div class="rule-priority-controls">
        <button
          type="button"
          class="rule-priority-btn"
          :disabled="!canMoveUp"
          aria-label="Increase priority"
          title="Increase priority"
          @click="emit('move-up')"
        >
          ▲
        </button>
        <button
          type="button"
          class="rule-priority-btn"
          :disabled="!canMoveDown"
          aria-label="Decrease priority"
          title="Decrease priority"
          @click="emit('move-down')"
        >
          ▼
        </button>
      </div>
      <div class="rule-name-meta">
        <strong>{{ rule.name }}</strong>
        <span class="rule-meta">Priority {{ rule.priority }}</span>
      </div>
    </td>

    <td>
      <ul class="rule-condition-lines">
        <li v-for="(condition, index) in rule.conditions" :key="`${rule.id}-${index}`">
          <span class="rule-condition-chip">
            <span class="rule-condition-label">{{ conditionTypeLabel(condition.type) }}</span>
            <span class="rule-condition-text">{{ formatConditionSummary(condition) }}</span>
          </span>
        </li>
      </ul>
    </td>

    <td>
      <div class="rule-target-line">
        <span class="rule-target-kind">{{ targetKindLabel }}</span>
        <span class="rule-target-name">{{ targetName }}</span>
      </div>
    </td>

    <td>
      <span
        class="rule-match-badge"
        :class="liveMatchCount > 0 ? 'rule-match-badge-active' : 'rule-match-badge-idle'"
      >
        {{ liveMatchCount > 0 ? `Matching ${liveMatchCount} now` : "No live match" }}
      </span>
    </td>

    <td>
      <ToggleSwitch
        :model-value="rule.enabled"
        :show-state-labels="false"
        @update:model-value="emit('toggle-enabled', $event)"
      />
    </td>

    <td class="rules-actions-cell">
      <div class="rule-card-actions">
        <button type="button" @click.stop="emit('edit')">Edit</button>
        <button type="button" class="danger" @click.stop="emit('delete')">Delete</button>
      </div>
    </td>
  </tr>
</template>
