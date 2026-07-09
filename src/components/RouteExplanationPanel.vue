<script setup lang="ts">
import { computed, ref } from "vue";
import type { Stream } from "../types/graph";
import { formatRuleLabel, routeExplanationSummary } from "../utils/routeExplanation";

const { stream, devices } = defineProps<{
  stream: Stream;
  devices: { id: string; label: string; system_name: string }[];
}>();

const expanded = ref(false);

const explanation = computed(() => stream.route_explanation);

const summary = computed(() => {
  const detail = explanation.value;
  if (!detail) {
    return "No routing explanation available";
  }

  const targetName =
    detail.target_system_name &&
    devices.find((device) => device.system_name === detail.target_system_name)?.label;

  return routeExplanationSummary(detail, targetName || undefined);
});

const statusLabel = computed(() => {
  const status = explanation.value?.action_status;
  switch (status) {
    case "applied":
      return "Applied";
    case "blocked":
      return "Blocked";
    case "skipped_manual_override":
      return "Skipped (manual override)";
    case "target_unavailable":
      return "Target unavailable";
    case "simulated":
      return "Would apply";
    default:
      return "No action";
  }
});

function focusRouteSelect() {
  const select = document.querySelector<HTMLSelectElement>(
    `[data-stream-route-select="${stream.id}"]`,
  );
  select?.focus();
}
</script>

<template>
  <div class="route-explanation">
    <button type="button" class="route-explanation-toggle" @click="expanded = !expanded">
      <span class="route-explanation-summary">{{ summary }}</span>
      <span class="route-explanation-chevron">{{ expanded ? "▾" : "▸" }}</span>
    </button>

    <div v-if="expanded && explanation" class="route-explanation-detail">
      <div class="route-explanation-row">
        <span class="route-explanation-label">Status</span>
        <span>{{ statusLabel }}</span>
      </div>

      <div v-if="explanation.match_reasons.length" class="route-explanation-row">
        <span class="route-explanation-label">Why</span>
        <ul>
          <li v-for="reason in explanation.match_reasons" :key="reason">{{ reason }}</li>
        </ul>
      </div>

      <div v-if="explanation.skipped_candidates.length" class="route-explanation-row">
        <span class="route-explanation-label">Skipped</span>
        <ul>
          <li
            v-for="candidate in explanation.skipped_candidates"
            :key="`${candidate.rule_key}-${candidate.reason}`"
          >
            {{ formatRuleLabel(candidate.rule_key) }}: {{ candidate.reason }}
          </li>
        </ul>
      </div>

      <button type="button" class="route-explanation-fix" @click="focusRouteSelect">
        Change route
      </button>
    </div>
  </div>
</template>
