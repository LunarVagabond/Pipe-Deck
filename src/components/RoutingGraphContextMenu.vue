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
  "add-node": [type: "bus" | "output" | "input"];
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
watch(
  () => props.target,
  () => {
    nodePickerOpen.value = false;
  },
);

function onPickNode(nodeId: string) {
  nodePickerOpen.value = false;
  emit("bring-node-here", nodeId);
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
      <button type="button" @click="emit('add-node', 'bus')">+ Bus</button>
      <button type="button" @click="emit('add-node', 'output')">+ Output (virtual)</button>
      <button type="button" @click="emit('add-node', 'input')">+ Virtual Input</button>

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
