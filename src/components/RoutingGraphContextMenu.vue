<script setup lang="ts">
import type { RoutingGraphMenuTarget } from "../composables/routingGraphContext";

defineProps<{
  target: RoutingGraphMenuTarget | null;
}>();

const emit = defineEmits<{
  rename: [];
  delete: [];
  clearEdgeReroutes: [];
  close: [];
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
      <button
        v-if="target.hasReroutes"
        type="button"
        @click="emit('clearEdgeReroutes')"
      >
        Clear reroute knots
      </button>
      <p v-else class="routing-graph-context-empty">No reroute knots on this wire</p>
    </template>
  </div>
</template>
