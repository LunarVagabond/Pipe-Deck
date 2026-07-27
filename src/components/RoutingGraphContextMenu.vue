<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { RoutingGraphMenuTarget } from "../composables/routingGraphContext";

/** Catalog of effects a node can attach — today just one kind, but this is
 * the reusable shape a second kind (parametric EQ #17, balance/pan #16,
 * dynamics once unblocked, ...) slots into without touching the menu's
 * structure again. */
interface AvailableEffect {
  kind: string;
  label: string;
}

const EFFECT_CATALOG: AvailableEffect[] = [{ kind: "eq5band", label: "5-Band EQ" }];

/** issue #293's non-DSP effect kinds — addable to the graph as visibly "Not
 * implemented yet" pass-through stub nodes (PD-032 phase 5), ahead of real
 * DSP landing for each in follow-up tickets. Originally 11; `reverb_delay`,
 * `limiter`, `hpf`, and `stereo_widener` graduated to real node buttons in
 * the General category below (issues #313/#311/#312/#314). */
const STUB_EFFECT_CATALOG: { kind: string; label: string }[] = [
  { kind: "compressor", label: "Compressor" },
  { kind: "noise_gate", label: "Noise Gate" },
  { kind: "denoise", label: "Noise Suppression" },
  { kind: "de_esser", label: "De-esser" },
  { kind: "auto_gain_leveler", label: "Auto Gain/Leveler" },
  { kind: "pitch_shift", label: "Pitch Shift/Voice Changer" },
  { kind: "loudness_normalizer", label: "Loudness Normalizer" },
  { kind: "saturation", label: "Saturation/Distortion" },
];

const props = defineProps<{
  target: RoutingGraphMenuTarget | null;
  /** Every node currently on the board — the source list for "Bring node
   * here" (issue #142). */
  nodes?: { id: string; label: string }[];
}>();

const emit = defineEmits<{
  rename: [];
  delete: [];
  "copy-id": [];
  close: [];
  "add-node": [
    type: "output" | "input" | "fan_out" | "mixer" | "eq5band" | "delay" | "limiter" | "hpf" | "reverb" | "widener",
  ];
  "add-stub-node": [stubKind: string, label: string];
  "add-effect": [kind: string];
  "bring-node-here": [nodeId: string];
}>();

const availableEffects = computed<AvailableEffect[]>(() => {
  const target = props.target;
  if (!target || target.kind !== "node" || !target.supportsEffects || !target.deviceId) {
    return [];
  }
  const existing = target.existingStageKinds ?? [];
  return EFFECT_CATALOG.filter((effect) => !existing.includes(effect.kind));
});

const nodePickerOpen = ref(false);
/** Which top-level "Add node" category flyout is open — only one at a time,
 * unlike `nodePickerOpen` (an unrelated, separate "Bring node here" picker
 * that can be open alongside). */
const openCategory = ref<"general" | "input" | "output" | null>(null);
watch(
  () => props.target,
  () => {
    nodePickerOpen.value = false;
    openCategory.value = null;
  },
);

function toggleCategory(category: "general" | "input" | "output") {
  openCategory.value = openCategory.value === category ? null : category;
}

function onPickNode(nodeId: string) {
  nodePickerOpen.value = false;
  emit("bring-node-here", nodeId);
}

function onPickNodeType(
  type: "output" | "input" | "fan_out" | "mixer" | "eq5band" | "delay" | "limiter" | "hpf" | "reverb" | "widener",
) {
  openCategory.value = null;
  emit("add-node", type);
}

function onPickStubEffect(stubKind: string, label: string) {
  openCategory.value = null;
  emit("add-stub-node", stubKind, label);
}
</script>

<template>
  <div
    v-if="target"
    class="routing-graph-context-menu"
    :style="{ left: `${target.x}px`, top: `${target.y}px` }"
    @mousedown.stop
    @pointerdown.stop
    @contextmenu.prevent
  >
    <template v-if="target.kind === 'node'">
      <button type="button" @click="emit('copy-id')">Copy ID</button>
      <hr
        v-if="target.editable || availableEffects.length > 0 || target.deletable"
        class="routing-graph-context-menu-separator"
      />

      <template v-if="target.editable">
        <button type="button" @click="emit('rename')">Rename</button>
        <hr v-if="availableEffects.length > 0 || target.deletable" class="routing-graph-context-menu-separator" />
      </template>

      <template v-if="availableEffects.length > 0">
        <p class="routing-graph-context-menu-label">Attach effect</p>
        <button
          v-for="effect in availableEffects"
          :key="effect.kind"
          type="button"
          @click="emit('add-effect', effect.kind)"
        >
          + {{ effect.label }}
        </button>
        <hr v-if="target.deletable" class="routing-graph-context-menu-separator" />
      </template>

      <button
        v-if="target.deletable"
        type="button"
        class="danger"
        @click="emit('delete')"
      >
        Delete
      </button>
    </template>
    <template v-else>
      <p class="routing-graph-context-menu-label">Add node</p>
      <div class="routing-graph-node-picker-anchor">
        <button type="button" @click="toggleCategory('general')">General ▸</button>
        <div v-if="openCategory === 'general'" class="routing-graph-node-category-flyout">
          <button type="button" @click="onPickNodeType('fan_out')">+ Fan-Out Node</button>
          <button type="button" @click="onPickNodeType('mixer')">+ Mixer Node</button>
          <button type="button" @click="onPickNodeType('eq5band')">+ 5-Band EQ Node</button>
          <button type="button" @click="onPickNodeType('delay')">+ Delay Node</button>
          <button type="button" @click="onPickNodeType('limiter')">+ Limiter Node</button>
          <button type="button" @click="onPickNodeType('hpf')">+ High-Pass Filter Node</button>
          <button type="button" @click="onPickNodeType('reverb')">+ Reverb Node</button>
          <button type="button" @click="onPickNodeType('widener')">+ Stereo Widener Node</button>
          <hr class="routing-graph-context-menu-separator" />
          <p class="routing-graph-context-menu-label">Not yet implemented</p>
          <button
            v-for="effect in STUB_EFFECT_CATALOG"
            :key="effect.kind"
            type="button"
            @click="onPickStubEffect(effect.kind, effect.label)"
          >
            {{ effect.label }}
          </button>
        </div>
      </div>
      <div class="routing-graph-node-picker-anchor">
        <button type="button" @click="toggleCategory('input')">Input ▸</button>
        <div v-if="openCategory === 'input'" class="routing-graph-node-category-flyout">
          <button type="button" @click="onPickNodeType('input')">+ Virtual Input</button>
        </div>
      </div>
      <div class="routing-graph-node-picker-anchor">
        <button type="button" @click="toggleCategory('output')">Output ▸</button>
        <div v-if="openCategory === 'output'" class="routing-graph-node-category-flyout">
          <button type="button" @click="onPickNodeType('output')">+ Virtual Output</button>
        </div>
      </div>

      <hr class="routing-graph-context-menu-separator" />
      <div class="routing-graph-node-picker-anchor">
        <button type="button" @click="nodePickerOpen = !nodePickerOpen">Bring node here…</button>
        <div v-if="nodePickerOpen" class="routing-graph-node-picker">
          <button
            v-for="node in nodes ?? []"
            :key="node.id"
            type="button"
            @click="onPickNode(node.id)"
          >
            {{ node.label }}
          </button>
          <p v-if="!nodes?.length" class="routing-graph-context-menu-label">No nodes on the board</p>
        </div>
      </div>
    </template>
  </div>
</template>
