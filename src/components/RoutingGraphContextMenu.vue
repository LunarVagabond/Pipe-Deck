<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import type { RoutingGraphMenuTarget } from "../composables/routingGraphContext";

type AddNodeType =
  | "output"
  | "input"
  | "fan_out"
  | "mixer"
  | "eq5band"
  | "delay"
  | "limiter"
  | "hpf"
  | "reverb"
  | "widener"
  | "pan";

/** Catalog of effects a node can attach — today just one kind, but this is
 * the reusable shape a second kind (parametric EQ #17, dynamics once
 * unblocked, ...) slots into without touching the menu's structure again. */
interface AvailableEffect {
  kind: string;
  label: string;
  description: string;
}

const EFFECT_CATALOG: AvailableEffect[] = [
  { kind: "eq5band", label: "5-Band EQ", description: "Shape tone across five adjustable frequency bands — boost bass, cut harshness, etc." },
];

/** Human-readable use-case blurb for each General-category processing node
 * kind, shown as a hover tooltip (issue: node descriptions in context menu)
 * so a user unfamiliar with terms like "Fan-Out" or "HPF" can see what a
 * node is actually for before adding it. */
const GENERAL_NODE_CATALOG: { type: AddNodeType; label: string; description: string }[] = [
  { type: "fan_out", label: "Fan-Out Node", description: "Duplicate one input to multiple destinations at once — e.g. play a track to your speakers and your recording software simultaneously." },
  { type: "mixer", label: "Mixer Node", description: "Combine multiple sources into one signal with independent volume per input — e.g. blend your mic and game audio into a single stream for Discord." },
  { type: "eq5band", label: "5-Band EQ Node", description: "Shape tone across five adjustable frequency bands — boost bass, cut harshness, etc." },
  { type: "delay", label: "Delay Node", description: "Add an adjustable echo/delay repeat to a signal." },
  { type: "limiter", label: "Limiter Node", description: "Cap peak loudness so a signal never exceeds a ceiling you set — prevents clipping on loud moments." },
  { type: "hpf", label: "High-Pass Filter Node", description: "Cut low-frequency rumble below a chosen cutoff — e.g. remove mic handling noise or desk thumps." },
  { type: "reverb", label: "Reverb Node", description: "Add spatial ambience/echo to a signal, like a room or hall." },
  { type: "widener", label: "Stereo Widener Node", description: "Widen the perceived stereo image of a signal for a bigger, more spacious sound." },
  { type: "pan", label: "Balance/Pan Node", description: "Shift a signal's balance between the left and right channels." },
];

/** issue #293's non-DSP effect kinds — addable to the graph as visibly "Not
 * implemented yet" pass-through stub nodes (PD-032 phase 5), ahead of real
 * DSP landing for each in follow-up tickets. Originally 11; `reverb_delay`,
 * `limiter`, `hpf`, and `stereo_widener` graduated to real node buttons in
 * the General category below (issues #313/#311/#312/#314). */
const STUB_EFFECT_CATALOG: { kind: string; label: string; description: string }[] = [
  { kind: "compressor", label: "Compressor", description: "Automatically reduce the difference between loud and quiet parts of a signal." },
  { kind: "noise_gate", label: "Noise Gate", description: "Mute a signal below a volume threshold — e.g. silence a mic between sentences to cut background noise." },
  { kind: "denoise", label: "Noise Suppression", description: "Suppress steady background noise in a signal — e.g. a fan or hum bleeding into a mic." },
  { kind: "de_esser", label: "De-esser", description: "Tame harsh sibilant \"s\"/\"sh\" sounds in vocals." },
  { kind: "auto_gain_leveler", label: "Auto Gain/Leveler", description: "Automatically adjust a signal's overall volume to stay consistent over time." },
  { kind: "pitch_shift", label: "Pitch Shift/Voice Changer", description: "Shift a signal's pitch up or down." },
  { kind: "loudness_normalizer", label: "Loudness Normalizer", description: "Normalize a signal to a consistent perceived loudness." },
  { kind: "saturation", label: "Saturation/Distortion", description: "Add warmth or grit by driving a signal into soft distortion." },
];

const IO_NODE_DESCRIPTIONS: Record<"input" | "output", string> = {
  input: "A virtual microphone-like device other apps can select as an input — e.g. combine sources to send into Discord.",
  output: "A virtual speaker-like device other apps can play to — e.g. route one app's audio through Pipe Deck before it reaches your speakers.",
};

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
  "add-node": [type: AddNodeType];
  "add-stub-node": [stubKind: string, label: string];
  "add-effect": [kind: string];
  "bring-node-here": [nodeId: string];
  "group-outputs": [];
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

/** A single searchable action in the pane menu's flat command list — every
 * category (general nodes, stub effects, input/output, "bring node here")
 * flattens into this shape so free-text search can rank across all of them
 * at once, letting a keyboard user skip the category flyouts entirely. */
interface SearchableAction {
  id: string;
  label: string;
  description?: string;
  run: () => void;
}

const searchQuery = ref("");
const searchInputRef = ref<HTMLInputElement | null>(null);
const highlightedIndex = ref(0);

const paneSearchActions = computed<SearchableAction[]>(() => {
  if (props.target?.kind !== "pane") {
    return [];
  }
  const actions: SearchableAction[] = [
    ...GENERAL_NODE_CATALOG.map((node) => ({
      id: `general-${node.type}`,
      label: `+ ${node.label}`,
      description: node.description,
      run: () => onPickNodeType(node.type),
    })),
    ...STUB_EFFECT_CATALOG.map((effect) => ({
      id: `stub-${effect.kind}`,
      label: effect.label,
      description: effect.description,
      run: () => onPickStubEffect(effect.kind, effect.label),
    })),
    {
      id: "io-input",
      label: "+ Virtual Input",
      description: IO_NODE_DESCRIPTIONS.input,
      run: () => onPickNodeType("input"),
    },
    {
      id: "io-output",
      label: "+ Virtual Output",
      description: IO_NODE_DESCRIPTIONS.output,
      run: () => onPickNodeType("output"),
    },
    ...(props.nodes ?? []).map((node) => ({
      id: `bring-${node.id}`,
      label: `Bring here: ${node.label}`,
      run: () => onPickNode(node.id),
    })),
  ];
  return actions;
});

const filteredSearchActions = computed<SearchableAction[]>(() => {
  const query = searchQuery.value.trim().toLowerCase();
  if (!query) {
    return [];
  }
  return paneSearchActions.value.filter(
    (action) =>
      action.label.toLowerCase().includes(query) ||
      action.description?.toLowerCase().includes(query),
  );
});

watch(filteredSearchActions, () => {
  highlightedIndex.value = 0;
});

function onSearchKeydown(event: KeyboardEvent) {
  const matches = filteredSearchActions.value;
  if (event.key === "ArrowDown") {
    event.preventDefault();
    if (matches.length > 0) {
      highlightedIndex.value = (highlightedIndex.value + 1) % matches.length;
    }
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    if (matches.length > 0) {
      highlightedIndex.value = (highlightedIndex.value - 1 + matches.length) % matches.length;
    }
  } else if (event.key === "Enter") {
    event.preventDefault();
    matches[highlightedIndex.value]?.run();
  } else if (event.key === "Escape") {
    event.preventDefault();
    if (searchQuery.value) {
      searchQuery.value = "";
    } else {
      emit("close");
    }
  }
}

const menuRef = ref<HTMLDivElement | null>(null);
/** Clamped-to-viewport render position — starts at the raw click point
 * (`target.x`/`target.y`), then nudged left/up after mount once the menu's
 * actual rendered size is known, so a right-click near the right or bottom
 * edge doesn't render the menu (or an expanded flyout) partly off-screen. */
const position = ref<{ left: number; top: number } | null>(null);

async function clampPosition() {
  const target = props.target;
  if (!target) {
    position.value = null;
    return;
  }
  position.value = { left: target.x, top: target.y };
  await nextTick();
  const el = menuRef.value;
  if (!el) return;
  const rect = el.getBoundingClientRect();
  const margin = 8;
  const maxLeft = window.innerWidth - rect.width - margin;
  const maxTop = window.innerHeight - rect.height - margin;
  position.value = {
    left: Math.max(margin, Math.min(target.x, maxLeft)),
    top: Math.max(margin, Math.min(target.y, maxTop)),
  };
}

watch(
  () => props.target,
  async () => {
    nodePickerOpen.value = false;
    openCategory.value = null;
    searchQuery.value = "";
    highlightedIndex.value = 0;
    clampPosition();
    if (props.target?.kind === "pane") {
      await nextTick();
      searchInputRef.value?.focus();
    }
  },
  { immediate: true },
);
// Expanding a category flyout or the "Bring node here" picker can grow the
// menu well past its initial closed size — re-clamp whenever either opens
// so the flyout itself doesn't run off-screen even when the closed menu fit.
// Typing into the pane search box swaps the categorized layout for a flat
// results list of a different size, so it needs the same re-clamp.
watch([openCategory, nodePickerOpen, searchQuery], clampPosition);

const menuStyle = computed(() => {
  const target = props.target;
  if (!target) return {};
  const resolved = position.value ?? { left: target.x, top: target.y };
  return { left: `${resolved.left}px`, top: `${resolved.top}px` };
});

function toggleCategory(category: "general" | "input" | "output") {
  openCategory.value = openCategory.value === category ? null : category;
}

function onPickNode(nodeId: string) {
  nodePickerOpen.value = false;
  emit("bring-node-here", nodeId);
}

function onPickNodeType(type: AddNodeType) {
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
    ref="menuRef"
    class="routing-graph-context-menu"
    :style="menuStyle"
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
          :title="effect.description"
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
    <template v-else-if="target.kind === 'multi-node'">
      <p class="routing-graph-context-menu-label">{{ target.memberLabels.length }} outputs selected</p>
      <button type="button" @click="emit('group-outputs')">Group Selected Outputs</button>
    </template>
    <template v-else>
      <input
        ref="searchInputRef"
        v-model="searchQuery"
        type="text"
        class="routing-graph-context-menu-search"
        placeholder="Search actions…"
        @keydown="onSearchKeydown"
      />
      <template v-if="searchQuery.trim()">
        <button
          v-for="(action, index) in filteredSearchActions"
          :key="action.id"
          type="button"
          :class="{ 'is-highlighted': index === highlightedIndex }"
          :title="action.description"
          @mouseenter="highlightedIndex = index"
          @click="action.run()"
        >
          {{ action.label }}
        </button>
        <p v-if="!filteredSearchActions.length" class="routing-graph-context-menu-label">No matches</p>
      </template>
      <template v-else>
        <p class="routing-graph-context-menu-label">Add node</p>
        <div class="routing-graph-node-picker-anchor">
          <button type="button" @click="toggleCategory('general')">General ▸</button>
          <div v-if="openCategory === 'general'" class="routing-graph-node-category-flyout">
            <button
              v-for="node in GENERAL_NODE_CATALOG"
              :key="node.type"
              type="button"
              :title="node.description"
              @click="onPickNodeType(node.type)"
            >
              + {{ node.label }}
            </button>
            <hr class="routing-graph-context-menu-separator" />
            <p class="routing-graph-context-menu-label">Not yet implemented</p>
            <button
              v-for="effect in STUB_EFFECT_CATALOG"
              :key="effect.kind"
              type="button"
              :title="effect.description"
              @click="onPickStubEffect(effect.kind, effect.label)"
            >
              {{ effect.label }}
            </button>
          </div>
        </div>
        <div class="routing-graph-node-picker-anchor">
          <button type="button" @click="toggleCategory('input')">Input ▸</button>
          <div v-if="openCategory === 'input'" class="routing-graph-node-category-flyout">
            <button type="button" :title="IO_NODE_DESCRIPTIONS.input" @click="onPickNodeType('input')">+ Virtual Input</button>
          </div>
        </div>
        <div class="routing-graph-node-picker-anchor">
          <button type="button" @click="toggleCategory('output')">Output ▸</button>
          <div v-if="openCategory === 'output'" class="routing-graph-node-category-flyout">
            <button type="button" :title="IO_NODE_DESCRIPTIONS.output" @click="onPickNodeType('output')">+ Virtual Output</button>
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
    </template>
  </div>
</template>
