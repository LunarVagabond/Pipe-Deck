<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import RuleFormModal from "../components/rules/RuleFormModal.vue";
import RuleListItem from "../components/rules/RuleListItem.vue";
import { useApplyResult } from "../stores/notices";
import { useConfirm } from "../stores/confirm";
import { useRuleDraft } from "../stores/ruleDraft";
import { useRuntimeGraph } from "../stores/runtimeGraph";
import type {
  Device,
  RecentStreamIdentity,
  Rule,
  SimulationResult,
  Stream,
} from "../types/graph";
import { formatConditionSummary } from "../utils/ruleConditions";
import {
  devicesForTargetKind,
  inferRuleTargetKind,
  ruleTargetKindLabel,
  targetLabel,
  type RuleTargetKind,
} from "../utils/routingLayout";
import {
  filterRecentlySeen,
  recentEntryAgo,
  recentEntryLabel,
} from "../utils/recentStreams";

const rules = ref<Rule[]>([]);
const simulation = ref<SimulationResult[]>([]);
const showSimulation = ref(false);
const priorityPreview = ref<Record<string, number>>({});
const showRuleModal = ref(false);
const editingRuleId = ref<string | null>(null);
const searchQuery = ref("");
const { handleApplyResult } = useApplyResult();
const { confirm } = useConfirm();
const { consumePendingIdentity } = useRuleDraft();
const { graph } = useRuntimeGraph();

const isEditing = computed(() => editingRuleId.value !== null);

const draft = ref<Rule>(emptyRule());

function emptyRule(): Rule {
  const targets = devicesForTargetKind(graph.value.devices, "output");
  return {
    id: crypto.randomUUID(),
    name: "New rule",
    enabled: true,
    priority: 10,
    conditions: [{ type: "identity", value: "" }],
    action: { target_system_name: targets[0]?.system_name ?? "" },
    safeguards: { fallback_policy: "keep_current" },
  };
}

function deviceBySystemName(systemName?: string): Device | undefined {
  return graph.value.devices.find(
    (device) => device.system_name === systemName,
  );
}

async function loadRules() {
  rules.value = await invoke<Rule[]>("list_rules");
}

async function refreshSimulation(reportErrors = false) {
  try {
    simulation.value = await invoke<SimulationResult[]>("simulate_rules");
  } catch (error) {
    if (reportErrors) {
      handleApplyResult(
        {
          success: false,
          message: error instanceof Error ? error.message : String(error),
        },
        "",
      );
    }
  }
}

const sortedRules = computed(() =>
  [...rules.value].sort((a, b) => b.priority - a.priority),
);

const filteredRules = computed(() => {
  const query = searchQuery.value.trim().toLowerCase();
  if (!query) {
    return sortedRules.value;
  }
  return sortedRules.value.filter((rule) => {
    if (rule.name.toLowerCase().includes(query)) {
      return true;
    }
    if (
      targetDisplay(rule.action.target_system_name)
        ?.toLowerCase()
        .includes(query)
    ) {
      return true;
    }
    return rule.conditions.some((condition) =>
      formatConditionSummary(condition).toLowerCase().includes(query),
    );
  });
});

function liveMatchCount(rule: Rule): number {
  return simulation.value.filter(
    (result) => result.explanation.matched_rule_key === rule.name,
  ).length;
}

interface ConflictLoser {
  ruleId?: string;
  ruleKey: string;
  rule?: Rule;
  reason: string;
}

interface ConflictEntry {
  streamId: string;
  streamLabel: string;
  winnerRuleId?: string;
  winnerRuleKey?: string;
  winnerRule?: Rule;
  losers: ConflictLoser[];
}

function ruleById(id?: string): Rule | undefined {
  return id ? rules.value.find((rule) => rule.id === id) : undefined;
}

const conflicts = computed<ConflictEntry[]>(() => {
  const entries: ConflictEntry[] = [];
  for (const result of simulation.value) {
    const losers = (result.explanation.skipped_candidates ?? []).filter(
      (candidate) => candidate.kind === "lower_priority",
    );
    if (losers.length === 0) {
      continue;
    }
    entries.push({
      streamId: result.stream_id,
      streamLabel: simulationLabel(result),
      winnerRuleId: result.explanation.matched_rule_id,
      winnerRuleKey: result.explanation.matched_rule_key,
      winnerRule: ruleById(result.explanation.matched_rule_id),
      losers: losers.map((candidate) => ({
        ruleId: candidate.rule_id,
        ruleKey: candidate.rule_key,
        rule: ruleById(candidate.rule_id),
        reason: candidate.reason,
      })),
    });
  }
  return entries;
});

const conflictingRuleIds = computed(() => {
  const ids = new Set<string>();
  for (const conflict of conflicts.value) {
    if (conflict.winnerRuleId) {
      ids.add(conflict.winnerRuleId);
    }
    for (const loser of conflict.losers) {
      if (loser.ruleId) {
        ids.add(loser.ruleId);
      }
    }
  }
  return ids;
});

function ruleHasConflict(rule: Rule): boolean {
  return conflictingRuleIds.value.has(rule.id);
}

// Preview simulation is fetched separately from `simulation` (which drives the
// stable conflict pairing above) because promoting a rule's priority can flip
// which rule wins a stream — feeding the override back into `simulation` would
// make the loser row a previewer clicked disappear (it becomes the winner) and
// replace it with the *previous* winner as a brand-new, never-clicked loser row.
const previewSimulation = ref<SimulationResult[] | null>(null);

function isPreviewed(ruleId?: string): boolean {
  return ruleId != null && ruleId in priorityPreview.value;
}

function previewedRuleResolved(streamId: string, ruleId: string): boolean {
  const result = previewSimulation.value?.find(
    (entry) => entry.stream_id === streamId,
  );
  return result?.explanation.matched_rule_id === ruleId;
}

async function refreshPreview() {
  if (Object.keys(priorityPreview.value).length === 0) {
    previewSimulation.value = null;
    return;
  }
  try {
    previewSimulation.value = await invoke<SimulationResult[]>(
      "simulate_rules",
      {
        priorityOverrides: priorityPreview.value,
      },
    );
  } catch (error) {
    handleApplyResult(
      {
        success: false,
        message: error instanceof Error ? error.message : String(error),
      },
      "",
    );
  }
}

async function previewPromote(loserRuleId: string, newPriority: number) {
  priorityPreview.value = {
    ...priorityPreview.value,
    [loserRuleId]: newPriority,
  };
  await refreshPreview();
}

async function clearPreview(ruleId?: string) {
  if (ruleId) {
    const rest = { ...priorityPreview.value };
    delete rest[ruleId];
    priorityPreview.value = rest;
  } else {
    priorityPreview.value = {};
  }
  await refreshPreview();
}

async function applyPriorityChange(rule: Rule, newPriority: number) {
  try {
    await invoke("save_rule", { rule: { ...rule, priority: newPriority } });
    const rest = { ...priorityPreview.value };
    delete rest[rule.id];
    priorityPreview.value = rest;
    await loadRules();
    await refreshPreview();
    await applyRules();
  } catch (error) {
    handleApplyResult(
      {
        success: false,
        message: error instanceof Error ? error.message : String(error),
      },
      "",
    );
  }
}

function openCreateModal() {
  editingRuleId.value = null;
  draft.value = emptyRule();
  showRuleModal.value = true;
}

function openCreateModalForIdentity(entry: RecentStreamIdentity) {
  editingRuleId.value = null;
  draft.value = emptyRule();
  const targetKind: RuleTargetKind =
    entry.direction === "capture" ? "input" : "output";
  const targets = devicesForTargetKind(graph.value.devices, targetKind);
  draft.value.action.target_system_name = targets[0]?.system_name ?? "";
  draft.value.name = `${recentEntryLabel(entry)} rule`;
  draft.value.conditions = [
    { type: "identity", value: entry.executable || entry.app_name },
    { type: "direction", value: entry.direction },
  ];
  showRuleModal.value = true;
}

const recentlySeenEntries = computed(() =>
  filterRecentlySeen(graph.value.recent_stream_identities),
);

function cloneRule(rule: Rule): Rule {
  return JSON.parse(JSON.stringify(rule)) as Rule;
}

function openEditModal(rule: Rule) {
  try {
    editingRuleId.value = rule.id;
    draft.value = cloneRule(rule);
    if (!draft.value.safeguards) {
      draft.value.safeguards = { fallback_policy: "keep_current" };
    }
    if (draft.value.conditions.length === 0) {
      draft.value.conditions = [{ type: "identity", value: "" }];
    }
    showRuleModal.value = true;
  } catch (error) {
    handleApplyResult(
      {
        success: false,
        message:
          error instanceof Error ? error.message : "Failed to open rule editor",
      },
      "",
    );
  }
}

function closeRuleModal() {
  showRuleModal.value = false;
  editingRuleId.value = null;
}

async function saveDraft() {
  const cleaned: Rule = {
    ...draft.value,
    name: draft.value.name.trim() || "Untitled rule",
    conditions: draft.value.conditions.filter((condition) =>
      (condition.type === "regex" ? condition.pattern : condition.value).trim(),
    ),
  };
  if (!cleaned.action.target_system_name) {
    handleApplyResult(
      { success: false, message: "Select a target device" },
      "",
    );
    return;
  }
  if (cleaned.conditions.length === 0) {
    handleApplyResult(
      { success: false, message: "Add at least one condition" },
      "",
    );
    return;
  }
  try {
    await invoke("save_rule", { rule: cleaned });
    handleApplyResult(
      { success: true },
      isEditing.value ? "Rule updated" : "Rule created",
    );
    closeRuleModal();
    draft.value = emptyRule();
    await loadRules();
    await refreshSimulation();
  } catch (error) {
    handleApplyResult(
      {
        success: false,
        message: error instanceof Error ? error.message : String(error),
      },
      "",
    );
  }
}

async function removeRule(rule: Rule) {
  const confirmed = await confirm(`Delete rule "${rule.name}"?`, {
    title: "Delete rule",
    confirmLabel: "Delete",
    cancelLabel: "Cancel",
  });
  if (!confirmed) {
    return;
  }

  try {
    await invoke("delete_rule", { ruleId: rule.id });
    handleApplyResult({ success: true }, "Rule deleted");
    await loadRules();
    await refreshSimulation();
  } catch (error) {
    handleApplyResult(
      {
        success: false,
        message: error instanceof Error ? error.message : String(error),
      },
      "",
    );
  }
}

async function toggleRuleEnabled(rule: Rule, enabled: boolean) {
  try {
    await invoke("toggle_rule", { ruleId: rule.id, enabled });
    await loadRules();
    await refreshSimulation();
  } catch (error) {
    handleApplyResult(
      {
        success: false,
        message: error instanceof Error ? error.message : String(error),
      },
      "",
    );
  }
}

async function moveRule(rule: Rule, direction: "up" | "down") {
  const sorted = sortedRules.value;
  const index = sorted.findIndex((entry) => entry.id === rule.id);
  const swapIndex = direction === "up" ? index - 1 : index + 1;
  if (index === -1 || swapIndex < 0 || swapIndex >= sorted.length) {
    return;
  }
  const other = sorted[swapIndex];
  try {
    await invoke("save_rule", { rule: { ...rule, priority: other.priority } });
    await invoke("save_rule", { rule: { ...other, priority: rule.priority } });
    await loadRules();
    await refreshSimulation();
  } catch (error) {
    handleApplyResult(
      {
        success: false,
        message: error instanceof Error ? error.message : String(error),
      },
      "",
    );
  }
}

async function applyRules() {
  try {
    const result = await invoke<{ success: boolean; message?: string }>(
      "apply_rules",
    );
    handleApplyResult(result, "Rules applied to PipeWire");
    await refreshSimulation();
  } catch (error) {
    handleApplyResult(
      {
        success: false,
        message: error instanceof Error ? error.message : String(error),
      },
      "",
    );
  }
}

async function runSimulation() {
  await refreshSimulation(true);
  showSimulation.value = true;
}

const visibleStreams = computed(() =>
  graph.value.streams.filter((stream) => !stream.is_system),
);

function recentIdentityToStream(entry: RecentStreamIdentity): Stream {
  return {
    id: `recent-${entry.app_name}-${entry.executable ?? "app"}`,
    app_name: entry.app_name,
    executable: entry.executable,
    window_class: entry.window_class,
    system_name: entry.system_name,
    direction: entry.direction,
    media_name: entry.media_name,
    is_system: entry.is_system,
  };
}

const recentIdentityStreams = computed(() =>
  (graph.value.recent_stream_identities ?? [])
    .filter((entry) => !entry.is_live && !entry.is_system)
    .map(recentIdentityToStream),
);

const identityStreams = computed(() => [
  ...visibleStreams.value,
  ...recentIdentityStreams.value,
]);

const recentIdentityIds = computed(
  () => new Set(recentIdentityStreams.value.map((stream) => stream.id)),
);

function streamLabel(streamId: string) {
  return (
    graph.value.streams.find((stream) => stream.id === streamId)?.app_name ??
    streamId
  );
}

function simulationLabel(result: SimulationResult) {
  return result.stream_label || streamLabel(result.stream_id);
}

function targetDisplay(systemName?: string) {
  const device = deviceBySystemName(systemName);
  return device ? targetLabel(device) : systemName;
}

function targetKindForSystemName(systemName?: string) {
  return ruleTargetKindLabel(
    inferRuleTargetKind(deviceBySystemName(systemName)),
  );
}

onMounted(async () => {
  await loadRules();
  await refreshSimulation();
  const pending = consumePendingIdentity();
  if (pending) {
    openCreateModalForIdentity(pending);
  }
});
</script>

<template>
  <div class="rules-view">
    <header class="rules-header view-header">
      <div class="rules-header-copy">
        <span class="rules-eyebrow">Advanced Routing</span>
        <h2>Auto-routing rules</h2>
        <p>
          Author deterministic rules and simulate outcomes. New apps are
          auto-routed when a rule matches (disable in Settings). Manual routing
          in Dashboard or Routing overrides rules for that session until you
          click Apply rules.
        </p>
      </div>
      <div class="rules-header-actions">
        <button type="button" class="primary" @click="applyRules">
          Apply rules
        </button>
        <button type="button" class="rules-new-btn" @click="openCreateModal">
          + New Rule
        </button>
        <button type="button" class="rules-simulate-btn" @click="runSimulation">
          Simulate
        </button>
      </div>
    </header>

    <section
      v-if="recentlySeenEntries.length > 0"
      class="rules-panel rules-panel-recent"
    >
      <div class="rules-panel-header">
        <div>
          <h3>Recently seen</h3>
          <p>
            Apps that briefly appeared in the last hour but aren't active right
            now — create a rule so they're routed correctly next time, even if
            they only last a second.
          </p>
        </div>
      </div>
      <ul class="recently-seen-list">
        <li
          v-for="(entry, index) in recentlySeenEntries"
          :key="`${entry.app_name}-${index}`"
        >
          <div class="recently-seen-info">
            <strong>{{ recentEntryLabel(entry) }}</strong>
            <span class="recently-seen-meta">
              {{ entry.direction === "capture" ? "Capture" : "Playback" }} ·
              {{ recentEntryAgo(entry) }}
            </span>
          </div>
          <button
            type="button"
            class="recently-seen-create-btn"
            @click="openCreateModalForIdentity(entry)"
          >
            Create rule
          </button>
        </li>
      </ul>
    </section>

    <section class="rules-panel rules-panel-list">
      <div class="rules-panel-header">
        <div>
          <h3>Configured Rules</h3>
          <p>
            Priority order is evaluated from highest to lowest. Use the arrows
            to reorder.
          </p>
        </div>
        <input
          v-if="rules.length > 0"
          v-model="searchQuery"
          type="search"
          class="rules-search-input"
          placeholder="Search rules…"
          aria-label="Search rules"
        />
        <span class="rules-count">{{ rules.length }} total</span>
      </div>

      <div v-if="rules.length === 0" class="rules-empty-state">
        <strong>No authored rules yet.</strong>
        <p>
          Click <strong>+ New Rule</strong> above to create your first routing
          policy.
        </p>
      </div>

      <div v-else-if="filteredRules.length === 0" class="rules-empty-state">
        <strong>No rules match "{{ searchQuery }}".</strong>
        <p>Try a different name, condition value, or target device.</p>
      </div>

      <div v-else class="rules-table-wrap">
        <table class="rules-table">
          <colgroup>
            <col class="rules-col-rule" />
            <col class="rules-col-conditions" />
            <col class="rules-col-target" />
            <col class="rules-col-match" />
            <col class="rules-col-status" />
            <col class="rules-col-actions" />
          </colgroup>
          <thead>
            <tr>
              <th>Rule</th>
              <th>Conditions</th>
              <th>Target</th>
              <th>Live match</th>
              <th>Enabled</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            <RuleListItem
              v-for="(rule, index) in filteredRules"
              :key="rule.id"
              :rule="rule"
              :target-kind-label="
                targetKindForSystemName(rule.action.target_system_name)
              "
              :target-name="targetDisplay(rule.action.target_system_name)"
              :live-match-count="liveMatchCount(rule)"
              :has-conflict="ruleHasConflict(rule)"
              :can-move-up="index > 0"
              :can-move-down="index < filteredRules.length - 1"
              @edit="openEditModal(rule)"
              @delete="removeRule(rule)"
              @toggle-enabled="toggleRuleEnabled(rule, $event)"
              @move-up="moveRule(rule, 'up')"
              @move-down="moveRule(rule, 'down')"
            />
          </tbody>
        </table>
      </div>
    </section>

    <section v-if="conflicts.length > 0" class="rules-panel rules-conflicts">
      <div class="rules-panel-header">
        <div>
          <h3>Conflicts</h3>
          <p>
            These streams matched more than one rule. The higher-priority rule
            wins; promote a lower rule to preview the resolution before applying
            it.
          </p>
        </div>
        <button
          v-if="Object.keys(priorityPreview).length > 0"
          type="button"
          class="rules-simulation-close"
          @click="clearPreview()"
        >
          Clear preview
        </button>
      </div>

      <article
        v-for="conflict in conflicts"
        :key="conflict.streamId"
        class="conflict-card"
      >
        <strong class="conflict-stream-label">{{
          conflict.streamLabel
        }}</strong>

        <div class="conflict-rules">
          <div class="conflict-rule conflict-rule-winner">
            <span class="conflict-rule-role">Winner</span>
            <strong>{{ conflict.winnerRuleKey }}</strong>
            <span v-if="conflict.winnerRule" class="rule-meta">
              Priority {{ conflict.winnerRule.priority }}
            </span>
            <ul v-if="conflict.winnerRule" class="rule-condition-lines">
              <li
                v-for="(condition, index) in conflict.winnerRule.conditions"
                :key="index"
              >
                {{ formatConditionSummary(condition) }}
              </li>
            </ul>
          </div>

          <div
            v-for="loser in conflict.losers"
            :key="`${conflict.streamId}-${loser.ruleId ?? loser.ruleKey}`"
            class="conflict-rule conflict-rule-loser"
          >
            <span class="conflict-rule-role">Skipped</span>
            <strong>{{ loser.ruleKey }}</strong>
            <span v-if="loser.rule" class="rule-meta"
              >Priority {{ loser.rule.priority }}</span
            >
            <ul v-if="loser.rule" class="rule-condition-lines">
              <li
                v-for="(condition, index) in loser.rule.conditions"
                :key="index"
              >
                {{ formatConditionSummary(condition) }}
              </li>
            </ul>
            <p class="conflict-reason">{{ loser.reason }}</p>

            <template v-if="loser.rule && loser.ruleId">
              <div v-if="isPreviewed(loser.ruleId)" class="conflict-actions">
                <span
                  class="conflict-preview-status"
                  :class="{
                    'conflict-resolved': previewedRuleResolved(
                      conflict.streamId,
                      loser.ruleId,
                    ),
                  }"
                >
                  {{
                    previewedRuleResolved(conflict.streamId, loser.ruleId)
                      ? "Resolved in preview"
                      : "Still conflicting in preview"
                  }}
                </span>
                <button
                  v-if="previewedRuleResolved(conflict.streamId, loser.ruleId)"
                  type="button"
                  class="primary"
                  @click="
                    applyPriorityChange(
                      loser.rule,
                      priorityPreview[loser.ruleId],
                    )
                  "
                >
                  Apply
                </button>
                <button type="button" @click="clearPreview(loser.ruleId)">
                  Cancel
                </button>
              </div>
              <button
                v-else
                type="button"
                class="conflict-promote-btn"
                @click="
                  previewPromote(
                    loser.ruleId,
                    (conflict.winnerRule?.priority ?? 0) + 1,
                  )
                "
              >
                Promote above "{{ conflict.winnerRuleKey }}"
              </button>
            </template>
          </div>
        </div>
      </article>
    </section>

    <RuleFormModal
      v-model="draft"
      :open="showRuleModal"
      :is-editing="isEditing"
      :devices="graph.devices"
      :identity-streams="identityStreams"
      :recent-identity-ids="recentIdentityIds"
      @save="saveDraft"
      @cancel="closeRuleModal"
    />

    <section v-if="showSimulation" class="rules-simulation">
      <div class="rules-simulation-header">
        <div>
          <h3>Simulation preview</h3>
          <p class="rules-simulation-help">
            Includes live streams and recently seen apps from the last hour.
            Internal clients like
            <code>pw-play</code> are excluded.
          </p>
        </div>
        <button
          type="button"
          class="rules-simulation-close"
          @click="showSimulation = false"
        >
          Close
        </button>
      </div>
      <p v-if="simulation.length === 0" class="identity-reference-empty">
        No matching streams in memory. Play audio in your app, wait a few
        seconds, then simulate again.
      </p>
      <article
        v-for="result in simulation"
        :key="result.stream_id"
        class="simulation-card"
      >
        <strong>
          {{ simulationLabel(result) }}
          <span v-if="result.is_recent" class="identity-recent-badge"
            >recent</span
          >
        </strong>
        <p>{{ result.explanation.match_reasons.join(", ") || "No match" }}</p>
        <p>
          Would route to
          {{
            targetDisplay(result.explanation.target_system_name) ?? "unchanged"
          }}
        </p>
      </article>
    </section>
  </div>
</template>
