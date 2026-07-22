<script setup lang="ts">
import { computed, ref, watch } from "vue";
import RuleConditionEditor from "./RuleConditionEditor.vue";
import type { Device, Rule, RuleCondition, Stream } from "../../types/graph";
import {
  liveSuggestionsForType,
  setConditionType,
  setConditionValue,
  streamFieldValue,
  type ConditionType,
} from "../../utils/ruleConditions";
import {
  deviceSubtitle,
  devicesForTargetKind,
  inferRuleTargetKind,
  targetLabel,
  type RuleTargetKind,
} from "../../utils/routingLayout";

const { open, isEditing, devices, identityStreams, recentIdentityIds } = defineProps<{
  open: boolean;
  isEditing: boolean;
  devices: Device[];
  identityStreams: Stream[];
  recentIdentityIds: Set<string>;
}>();

const rule = defineModel<Rule>({ required: true });

const emit = defineEmits<{
  save: [];
  cancel: [];
}>();

const targetKind = ref<RuleTargetKind>("output");
const activeConditionIndex = ref(0);
const showIdentityReference = ref(true);

watch(
  () => open,
  (isOpen) => {
    if (!isOpen) return;
    targetKind.value = inferRuleTargetKind(
      devices.find((device) => device.system_name === rule.value.action.target_system_name),
    );
    activeConditionIndex.value = 0;
    showIdentityReference.value = true;
  },
  { immediate: true },
);

const filteredTargetDevices = computed(() => devicesForTargetKind(devices, targetKind.value));

watch(targetKind, (kind) => {
  const targets = devicesForTargetKind(devices, kind);
  const current = devices.find((device) => device.system_name === rule.value.action.target_system_name);
  const stillValid = current && targets.some((device) => device.id === current.id);
  if (!stillValid) {
    rule.value.action.target_system_name = targets[0]?.system_name ?? "";
  }
});

const targetKindHint = computed(() =>
  targetKind.value === "output"
    ? "Playback targets: speakers, headphones, and virtual sinks."
    : "Input targets: microphones, virtual inputs, and virtual mics.",
);

function addCondition() {
  rule.value.conditions.push({ type: "executable", value: "" });
}

function removeCondition(index: number) {
  if (rule.value.conditions.length <= 1) return;
  rule.value.conditions.splice(index, 1);
}

function suggestionsForCondition(condition: RuleCondition) {
  return liveSuggestionsForType(identityStreams, condition.type);
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
  if (!value) return "—";
  if (type === "direction") return value === "capture" ? "Capture" : "Playback";
  return value;
}

function useIdentityValue(type: ConditionType, value: string) {
  const condition = rule.value.conditions[activeConditionIndex.value];
  if (!condition || !value || value === "—" || type === "regex") return;
  setConditionType(condition, type);
  const normalized = type === "direction" ? (value === "Capture" ? "capture" : "playback") : value;
  setConditionValue(condition, normalized);
}
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="rules-modal" @click.self="emit('cancel')">
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
          <button type="button" class="rules-modal-close" aria-label="Close" @click="emit('cancel')">
            ×
          </button>
        </header>

        <div class="rules-editor">
          <div class="rules-form-grid">
            <label>
              <span class="field-label">Name</span>
              <input v-model="rule.name" type="text" />
            </label>
            <label>
              <span class="field-label">Priority</span>
              <input v-model.number="rule.priority" type="number" />
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
                :class="{ active: targetKind === 'output' }"
                @click="targetKind = 'output'"
              >
                Output
              </button>
              <button
                type="button"
                class="target-kind-option"
                :class="{ active: targetKind === 'input' }"
                @click="targetKind = 'input'"
              >
                Input
              </button>
            </div>

            <label>
              <span class="field-label">Target device</span>
              <select v-model="rule.action.target_system_name" :disabled="filteredTargetDevices.length === 0">
                <option v-if="filteredTargetDevices.length === 0" value="" disabled>
                  No {{ targetKind }} targets available
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
              <select v-model="rule.safeguards.fallback_policy">
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

            <RuleConditionEditor
              v-for="(condition, index) in rule.conditions"
              :key="`draft-${index}`"
              :condition="condition"
              :active="activeConditionIndex === index"
              :can-remove="rule.conditions.length > 1"
              :suggestions="suggestionsForCondition(condition)"
              @activate="activeConditionIndex = index"
              @remove="removeCondition(index)"
            />

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
                        :class="{ recent: recentIdentityIds.has(stream.id) }"
                      >
                        <td class="identity-app-cell">
                          {{ stream.app_name }}
                          <span v-if="recentIdentityIds.has(stream.id)" class="identity-recent-badge">
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
          <button type="button" @click="emit('cancel')">Cancel</button>
          <button type="button" class="primary" @click="emit('save')">
            {{ isEditing ? "Save changes" : "Save rule" }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
