<script setup lang="ts">
import type { RoutingGraphMenuTarget } from "../composables/routingGraphContext";

defineProps<{
  target: RoutingGraphMenuTarget | null;
}>();

const emit = defineEmits<{
  rename: [];
  delete: [];
  close: [];
  "add-node": [type: "output" | "output-multi" | "input"];
}>();
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
      <button
        v-if="target.editable"
        type="button"
        @click="emit('rename')"
      >
        Rename
      </button>
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
      <button type="button" @click="emit('add-node', 'output')">+ Virtual Output</button>
      <button type="button" @click="emit('add-node', 'output-multi')">+ Virtual Multi Output</button>
      <button type="button" @click="emit('add-node', 'input')">+ Virtual Input</button>
    </template>
  </div>
</template>
