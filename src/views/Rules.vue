<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useApplyResult } from "../stores/notices";
import { useConfirm } from "../stores/confirm";
import { useRuleDraft } from "../stores/ruleDraft";
import { useRuntimeGraph } from "../stores/runtimeGraph";
import type { Device, RecentStreamIdentity, Rule, RuleCondition, SimulationResult, Stream } from "../types/graph";
import {
  CATEGORY_OPTIONS,
  CONDITION_TYPE_OPTIONS,
  DIRECTION_OPTIONS,
  REGEX_FIELD_OPTIONS,
  conditionTypeLabel,
  conditionTypeMeta,
  formatConditionSummary,
  liveSuggestionsForType,
  streamFieldValue,
  type ConditionType,
} from "../utils/ruleConditions";
import {
  deviceSubtitle,
  devicesForTargetKind,
  inferRuleTargetKind,
  ruleTargetKindLabel,
  targetLabel,
  type RuleTargetKind,
} from "../utils/routingLayout";
import { filterRecentlySeen, recentEntryAgo, recentEntryLabel } from "../utils/recentStreams";

const rules = ref<Rule[]>([]);
const simulation = ref<SimulationResult[]>([]);
const showSimulation = ref(false);
const showRuleModal = ref(false);
const editingRuleId = ref<string | null>(null);
const draftTargetKind = ref<RuleTargetKind>("output");
const { handleApplyResult } = useApplyResult();
const { confirm } = useConfirm();
const { consumePendingIdentity } = useRuleDraft();
const { graph } = useRuntimeGraph();

const isEditing = computed(() => editingRuleId.value !== null);

const filteredTargetDevices = computed(() =>
  devicesForTargetKind(graph.value.devices, draftTargetKind.value),
);

const draft = ref<Rule>(emptyRule());

function emptyRule(): Rule {
  const targets = devicesForTargetKind(graph.value.devices, draftTargetKind.value);
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
  return graph.value.devices.find((device) => device.system_name === systemName);
}

function setDraftTargetKind(kind: RuleTargetKind) {
  draftTargetKind.value = kind;
}

watch(draftTargetKind, (kind) => {
  const targets = devicesForTargetKind(graph.value.devices, kind);
  const current = deviceBySystemName(draft.value.action.target_system_name);
  const stillValid = current && targets.some((device) => device.id === current.id);
  if (!stillValid) {
    draft.value.action.target_system_name = targets[0]?.system_name ?? "";
  }
});

async function loadRules() {
  rules.value = await invoke<Rule[]>("list_rules");
}

function openCreateModal() {
  editingRuleId.value = null;
  draft.value = emptyRule();
  draftTargetKind.value = "output";
  activeConditionIndex.value = 0;
  showIdentityReference.value = true;
  showRuleModal.value = true;
}

function openCreateModalForIdentity(entry: RecentStreamIdentity) {
  openCreateModal();
  draftTargetKind.value = entry.direction === "capture" ? "input" : "output";
  draft.value.name = `${recentEntryLabel(entry)} rule`;
  draft.value.conditions = [
    { type: "identity", value: entry.executable || entry.app_name },
    { type: "direction", value: entry.direction },
  ];
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
    draftTargetKind.value = inferRuleTargetKind(
      deviceBySystemName(rule.action.target_system_name),
    );
    activeConditionIndex.value = 0;
    showIdentityReference.value = true;
    showRuleModal.value = true;
  } catch (error) {
    handleApplyResult(
      {
        success: false,
        message: error instanceof Error ? error.message : "Failed to open rule editor",
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
    conditions: draft.value.conditions.filter((condition) => conditionValue(condition).trim()),
  };
  if (!cleaned.action.target_system_name) {
    handleApplyResult({ success: false, message: "Select a target device" }, "");
    return;
  }
  if (cleaned.conditions.length === 0) {
    handleApplyResult({ success: false, message: "Add at least one condition" }, "");
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
    draftTargetKind.value = "output";
    await loadRules();
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
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
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

async function toggleRule(rule: Rule) {
  try {
    await invoke("toggle_rule", { ruleId: rule.id, enabled: !rule.enabled });
    await loadRules();
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

async function applyRules() {
  try {
    const result = await invoke<{ success: boolean; message?: string }>("apply_rules");
    handleApplyResult(result, "Rules applied to PipeWire");
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

async function runSimulation() {
  try {
    simulation.value = await invoke<SimulationResult[]>("simulate_rules");
    showSimulation.value = true;
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

function conditionValue(condition: RuleCondition): string {
  if (condition.type === "regex") {
    return condition.pattern;
  }
  return condition.value;
}

function setConditionValue(condition: RuleCondition, value: string) {
  if (condition.type === "regex") {
    condition.pattern = value;
    return;
  }
  condition.value = value;
}

function addCondition() {
  draft.value.conditions.push({ type: "executable", value: "" });
}

function removeCondition(index: number) {
  if (draft.value.conditions.length <= 1) {
    return;
  }
  draft.value.conditions.splice(index, 1);
}

function applySuggestion(condition: RuleCondition, value: string) {
  setConditionValue(condition, value);
}

function suggestionsForCondition(condition: RuleCondition) {
  return liveSuggestionsForType(identityStreams.value, condition.type);
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

const recentIdentityKeys = computed(
  () => new Set(recentIdentityStreams.value.map((stream) => stream.id)),
);

function isRecentIdentityStream(stream: Stream) {
  return recentIdentityKeys.value.has(stream.id);
}

const showIdentityReference = ref(true);
const activeConditionIndex = ref(0);

function setActiveCondition(index: number) {
  activeConditionIndex.value = index;
}

const identityColumns = [
  { type: "app_name" as const, label: "App Name" },
  { type: "executable" as const, label: "Executable" },
  { type: "media_name" as const, label: "Media Name" },
  { type: "window_class" as const, label: "Window Class" },
  { type: "direction" as const, label: "Direction" },
];

function identityCellValue(stream: Stream, type: ConditionType): string {
  const value = streamFieldValue(stream, type);
  if (!value) {
    return "—";
  }
  if (type === "direction") {
    return value === "capture" ? "Capture" : "Playback";
  }
  return value;
}

function setConditionType(condition: RuleCondition, type: ConditionType) {
  if (type === "regex") {
    Object.assign(condition, { type, field: "app_name", pattern: "" });
    return;
  }
  if (type === "direction") {
    Object.assign(condition, { type, value: "playback" });
    return;
  }
  if (type === "category") {
    Object.assign(condition, { type, value: "Game" });
    return;
  }
  if (type === "identity") {
    Object.assign(condition, { type, value: "" });
    return;
  }
  Object.assign(condition, { type, value: "" });
}

function useIdentityValue(type: ConditionType, value: string) {
  const condition = draft.value.conditions[activeConditionIndex.value];
  if (!condition || !value || value === "—" || type === "regex") {
    return;
  }
  setConditionType(condition, type);
  const normalized =
    type === "direction" ? (value === "Capture" ? "capture" : "playback") : value;
  setConditionValue(condition, normalized);
}

function streamLabel(streamId: string) {
  return graph.value.streams.find((stream) => stream.id === streamId)?.app_name ?? streamId;
}

function simulationLabel(result: SimulationResult) {
  return result.stream_label || streamLabel(result.stream_id);
}

function targetDisplay(systemName?: string) {
  const device = deviceBySystemName(systemName);
  return device ? targetLabel(device) : systemName;
}

function targetKindForSystemName(systemName?: string) {
  return ruleTargetKindLabel(inferRuleTargetKind(deviceBySystemName(systemName)));
}

const targetKindHint = computed(() =>
  draftTargetKind.value === "output"
    ? "Playback targets: speakers, headphones, and virtual sinks."
    : "Input targets: microphones, virtual inputs, and virtual mics.",
);

onMounted(async () => {
  await loadRules();
  const pending = consumePendingIdentity();
  if (pending) {
    openCreateModalForIdentity(pending);
  }
});
</script>

<template>
  <div class="rules-view">
    <header class="rules-header">
      <div class="rules-header-copy">
        <span class="rules-eyebrow">Advanced Routing</span>
        <h2>Auto-routing rules</h2>
        <p>
          Author deterministic rules and simulate outcomes. New apps are auto-routed when a rule
          matches (disable in Settings). Manual routing in Dashboard or Routing overrides rules for
          that session until you click Apply rules.
        </p>
      </div>
      <div class="rules-header-actions">
        <button type="button" class="primary" @click="applyRules">Apply rules</button>
        <button type="button" class="rules-new-btn" @click="openCreateModal">+ New Rule</button>
        <button type="button" class="rules-simulate-btn" @click="runSimulation">Simulate</button>
      </div>
    </header>

    <section v-if="recentlySeenEntries.length > 0" class="rules-panel rules-panel-recent">
      <div class="rules-panel-header">
        <div>
          <h3>Recently seen</h3>
          <p>
            Apps that briefly appeared in the last hour but aren't active right now — create a rule
            so they're routed correctly next time, even if they only last a second.
          </p>
        </div>
      </div>
      <ul class="recently-seen-list">
        <li v-for="(entry, index) in recentlySeenEntries" :key="`${entry.app_name}-${index}`">
          <div class="recently-seen-info">
            <strong>{{ recentEntryLabel(entry) }}</strong>
            <span class="recently-seen-meta">
              {{ entry.direction === "capture" ? "Capture" : "Playback" }} · {{ recentEntryAgo(entry) }}
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
          <p>Priority order is evaluated from highest to lowest.</p>
        </div>
        <span class="rules-count">{{ rules.length }} total</span>
      </div>

      <div v-if="rules.length === 0" class="rules-empty-state">
        <strong>No authored rules yet.</strong>
        <p>Click <strong>+ New Rule</strong> above to create your first routing policy.</p>
      </div>

      <div v-else class="rules-table-wrap">
        <table class="rules-table">
          <colgroup>
            <col class="rules-col-rule" />
            <col class="rules-col-conditions" />
            <col class="rules-col-target" />
            <col class="rules-col-status" />
            <col class="rules-col-actions" />
          </colgroup>
          <thead>
            <tr>
              <th>Rule</th>
              <th>Conditions</th>
              <th>Target</th>
              <th>Status</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="rule in rules" :key="rule.id">
              <td class="rules-rule-cell">
                <strong>{{ rule.name }}</strong>
                <span class="rule-meta">Priority {{ rule.priority }}</span>
              </td>

              <td>
                <ul class="rule-condition-lines">
                  <li v-for="(condition, index) in rule.conditions" :key="`${rule.id}-${index}`">
                    <span class="rule-condition-label">{{ conditionTypeLabel(condition.type) }}</span>
                    <span class="rule-condition-text">{{ formatConditionSummary(condition) }}</span>
                  </li>
                </ul>
              </td>

              <td>
                <div class="rule-target-line">
                  <span class="rule-target-kind">{{ targetKindForSystemName(rule.action.target_system_name) }}</span>
                  <span class="rule-target-name">{{ targetDisplay(rule.action.target_system_name) }}</span>
                </div>
              </td>

              <td>
                <span class="rule-status-text" :class="{ disabled: !rule.enabled }">
                  {{ rule.enabled ? "Enabled" : "Disabled" }}
                </span>
              </td>

              <td class="rules-actions-cell">
                <div class="rule-card-actions">
                  <button type="button" @click.stop="openEditModal(rule)">Edit</button>
                  <button type="button" @click.stop="toggleRule(rule)">
                    {{ rule.enabled ? "Disable" : "Enable" }}
                  </button>
                  <button type="button" class="danger" @click.stop="removeRule(rule)">Delete</button>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>

    <Teleport to="body">
      <div
        v-if="showRuleModal"
        class="rules-modal"
        @click.self="closeRuleModal"
      >
        <div
          class="rules-modal-dialog"
          role="dialog"
          aria-modal="true"
          :aria-labelledby="isEditing ? 'edit-rule-title' : 'create-rule-title'"
        >
          <header class="rules-modal-header">
            <div>
              <h2 :id="isEditing ? 'edit-rule-title' : 'create-rule-title'">
                {{ isEditing ? "Edit rule" : "Create rule" }}
              </h2>
              <p>
                {{
                  isEditing
                    ? "Update the rule name, conditions, target, or priority."
                    : "Define the app signal, choose a target, and save it as a reusable policy."
                }}
              </p>
            </div>
            <button
              type="button"
              class="rules-modal-close"
              aria-label="Close"
              @click="closeRuleModal"
            >
              ×
            </button>
          </header>

          <div class="rules-editor">
            <div class="rules-form-grid">
              <label>
                <span class="field-label">Name</span>
                <input v-model="draft.name" type="text" />
              </label>
              <label>
                <span class="field-label">Priority</span>
                <input v-model.number="draft.priority" type="number" />
              </label>
            </div>

            <div class="rules-target-section">
              <div class="rules-target-header">
                <span class="field-label">Route to</span>
                <p class="rules-target-hint">{{ targetKindHint }}</p>
              </div>

              <div class="target-kind-switch" role="group" aria-label="Target type">
                <button
                  type="button"
                  class="target-kind-option"
                  :class="{ active: draftTargetKind === 'output' }"
                  @click="setDraftTargetKind('output')"
                >
                  Output
                </button>
                <button
                  type="button"
                  class="target-kind-option"
                  :class="{ active: draftTargetKind === 'input' }"
                  @click="setDraftTargetKind('input')"
                >
                  Input
                </button>
              </div>

              <label>
                <span class="field-label">Target device</span>
                <select v-model="draft.action.target_system_name" :disabled="filteredTargetDevices.length === 0">
                  <option v-if="filteredTargetDevices.length === 0" value="" disabled>
                    No {{ draftTargetKind }} targets available
                  </option>
                  <option
                    v-for="device in filteredTargetDevices"
                    :key="device.system_name"
                    :value="device.system_name"
                  >
                    {{ targetLabel(device) }} — {{ deviceSubtitle(device) }}
                  </option>
                </select>
              </label>

              <label>
                <span class="field-label">If target unavailable</span>
                <select v-model="draft.safeguards.fallback_policy">
                  <option value="keep_current">Keep current route</option>
                  <option value="safe_default">Route to safe default device</option>
                </select>
              </label>
            </div>

            <div class="rule-conditions-editor">
              <div class="rule-conditions-editor-header">
                <div>
                  <h4>Conditions</h4>
                  <p>All entered conditions must match.</p>
                </div>
                <button type="button" class="secondary" @click="addCondition">Add condition</button>
              </div>

              <div
                v-for="(condition, index) in draft.conditions"
                :key="`draft-${index}`"
                class="condition-card"
                :class="{ active: activeConditionIndex === index }"
                @click="setActiveCondition(index)"
              >
                <div class="condition-row">
                  <label class="condition-field">
                    <span class="field-label">Match by</span>
                    <select
                      :value="condition.type"
                      @change="
                        setConditionType(
                          condition,
                          ($event.target as HTMLSelectElement).value as ConditionType,
                        )
                      "
                    >
                      <option
                        v-for="option in CONDITION_TYPE_OPTIONS"
                        :key="option.type"
                        :value="option.type"
                      >
                        {{ option.label }}
                      </option>
                    </select>
                  </label>

                  <template v-if="condition.type === 'regex'">
                    <label class="condition-field">
                      <span class="field-label">Field</span>
                      <select v-model="condition.field">
                        <option
                          v-for="option in REGEX_FIELD_OPTIONS"
                          :key="option.value"
                          :value="option.value"
                        >
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
                        <option
                          v-for="option in DIRECTION_OPTIONS"
                          :key="option.value"
                          :value="option.value"
                        >
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
                    :disabled="draft.conditions.length <= 1"
                    @click.stop="removeCondition(index)"
                  >
                    Remove
                  </button>
                </div>

                <p class="condition-help">
                  {{ conditionTypeMeta(condition.type).description }}
                  <span class="condition-example">
                    Example: {{ conditionTypeMeta(condition.type).example }}
                  </span>
                </p>

                <div v-if="suggestionsForCondition(condition).length" class="condition-suggestions">
                  <span class="condition-suggestions-label">From active audio:</span>
                  <button
                    v-for="value in suggestionsForCondition(condition)"
                    :key="`${condition.type}-${value}`"
                    type="button"
                    class="condition-suggestion-chip"
                    @click="applySuggestion(condition, value)"
                  >
                    {{ value }}
                  </button>
                </div>
              </div>

              <section class="identity-reference">
                <button
                  type="button"
                  class="identity-reference-toggle"
                  @click="showIdentityReference = !showIdentityReference"
                >
                  <span>Identify app values</span>
                  <span>{{ showIdentityReference ? "Hide" : "Show" }}</span>
                </button>

                <div v-if="showIdentityReference" class="identity-reference-body">
                  <p>
                    Compare how PipeWire labels each stream. Live rows update in real time; recently
                    seen rows stay for about an hour after a stream disappears (e.g. while you adjust
                    volume). Rule the app that is actually playing — not internal clients like
                    <code>pw-play</code>.
                  </p>

                  <div v-if="identityStreams.length === 0" class="identity-reference-empty">
                    No streams seen yet. Start audio in an app, or change system volume once, then
                    check back here.
                  </div>

                  <div v-else class="identity-reference-table-wrap">
                    <table class="identity-reference-table">
                      <thead>
                        <tr>
                          <th>App</th>
                          <th>App Name</th>
                          <th>Executable</th>
                          <th>Media Name</th>
                          <th>Window Class</th>
                          <th>Direction</th>
                        </tr>
                      </thead>
                      <tbody>
                        <tr
                          v-for="stream in identityStreams"
                          :key="stream.id"
                          :class="{ recent: isRecentIdentityStream(stream) }"
                        >
                          <td class="identity-app-cell">
                            {{ stream.app_name }}
                            <span v-if="isRecentIdentityStream(stream)" class="identity-recent-badge">
                              recent
                            </span>
                          </td>
                          <td v-for="column in identityColumns" :key="`${stream.id}-${column.type}`">
                            <button
                              type="button"
                              class="identity-value-btn"
                              :disabled="identityCellValue(stream, column.type) === '—'"
                              @click="useIdentityValue(column.type, identityCellValue(stream, column.type))"
                            >
                              {{ identityCellValue(stream, column.type) }}
                            </button>
                          </td>
                        </tr>
                      </tbody>
                    </table>
                  </div>
                </div>
              </section>
            </div>
          </div>

          <div class="rules-modal-actions">
            <button type="button" @click="closeRuleModal">Cancel</button>
            <button type="button" class="primary" @click="saveDraft">
              {{ isEditing ? "Save changes" : "Save rule" }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <section v-if="showSimulation" class="rules-simulation">
      <div class="rules-simulation-header">
        <div>
          <h3>Simulation preview</h3>
          <p class="rules-simulation-help">
            Includes live streams and recently seen apps from the last hour. Internal clients like
            <code>pw-play</code> are excluded.
          </p>
        </div>
        <button type="button" class="rules-simulation-close" @click="showSimulation = false">
          Close
        </button>
      </div>
      <p v-if="simulation.length === 0" class="identity-reference-empty">
        No matching streams in memory. Play audio in your app, wait a few seconds, then simulate again.
      </p>
      <article
        v-for="result in simulation"
        :key="result.stream_id"
        class="simulation-card"
      >
        <strong>
          {{ simulationLabel(result) }}
          <span v-if="result.is_recent" class="identity-recent-badge">recent</span>
        </strong>
        <p>{{ result.explanation.match_reasons.join(", ") || "No match" }}</p>
        <p>
          Would route to
          {{ targetDisplay(result.explanation.target_system_name) ?? "unchanged" }}
        </p>
      </article>
    </section>
  </div>
</template>
