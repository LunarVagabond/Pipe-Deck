<script setup lang="ts">
import { computed, ref } from "vue";
import type { Stream } from "../types/graph";
import { actionStatusLabel, formatRuleLabel, routeExplanationSummary } from "../utils/routeExplanation";

const { stream, devices } = defineProps<{
  stream: Stream;
  devices: { id: string; label: string; system_name: string }[];
}>();

const expanded = ref(false);

const toggleId = computed(() => `route-explanation-toggle-${stream.id}`);
const detailId = computed(() => `route-explanation-detail-${stream.id}`);

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

const statusLabel = computed(() => actionStatusLabel(explanation.value?.action_status));

function focusRouteSelect() {
  const select = document.querySelector<HTMLSelectElement>(
    `[data-stream-route-select="${stream.id}"]`,
  );
  select?.focus();
}
</script>

<template>
  <div class="route-explanation">
    <button
      :id="toggleId"
      type="button"
      class="route-explanation-toggle"
      :aria-expanded="expanded"
      :aria-controls="detailId"
      @click="expanded = !expanded"
    >
      <span class="route-explanation-summary">{{ summary }}</span>
      <span class="route-explanation-chevron" aria-hidden="true">{{ expanded ? "▾" : "▸" }}</span>
    </button>

    <div
      v-if="expanded && explanation"
      :id="detailId"
      class="route-explanation-detail"
      role="region"
      :aria-labelledby="toggleId"
    >
      <dl class="route-explanation-list">
        <div class="route-explanation-row">
          <dt class="route-explanation-label">Status</dt>
          <dd>{{ statusLabel }}</dd>
        </div>

        <div v-if="explanation.fallback_applied" class="route-explanation-row">
          <dt class="route-explanation-label">Fallback</dt>
          <dd><span class="route-explanation-fallback-badge">Safe-default fallback applied</span></dd>
        </div>

        <div v-if="explanation.match_reasons.length" class="route-explanation-row">
          <dt class="route-explanation-label">Why</dt>
          <dd>
            <ul>
              <li v-for="reason in explanation.match_reasons" :key="reason">{{ reason }}</li>
            </ul>
          </dd>
        </div>

        <div v-if="explanation.skipped_candidates.length" class="route-explanation-row">
          <dt class="route-explanation-label">Skipped</dt>
          <dd>
            <ul>
              <li
                v-for="candidate in explanation.skipped_candidates"
                :key="`${candidate.rule_key}-${candidate.reason}`"
              >
                {{ formatRuleLabel(candidate.rule_key) }}: {{ candidate.reason }}
              </li>
            </ul>
          </dd>
        </div>
      </dl>

      <button type="button" class="route-explanation-fix" @click="focusRouteSelect">
        Change route
      </button>
    </div>
  </div>
</template>
