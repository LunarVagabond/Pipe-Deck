<script setup lang="ts">
import type { RoutingGraphMenuTarget } from "../composables/routingGraphContext";

defineProps<{
  target: RoutingGraphMenuTarget | null;
}>();

const emit = defineEmits<{
  rename: [];
  delete: [];
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
  </div>
</template>
