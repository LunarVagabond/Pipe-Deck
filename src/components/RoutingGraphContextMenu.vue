<script setup lang="ts">
import type { RoutingGraphMenuTarget } from "../composables/routingGraphContext";

defineProps<{
  target: RoutingGraphMenuTarget | null;
}>();

const emit = defineEmits<{
  rename: [];
  delete: [];
  close: [];
  "add-node": [type: "output" | "input"];
  "add-effect": [sourceId: string, targetId: string];
  "remove-effect": [sourceId: string, targetId: string];
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
      <template v-if="target.connections.length">
        <p v-if="target.editable || target.deletable" class="routing-graph-context-menu-label">
          Connections
        </p>
        <button
          v-for="connection in target.connections"
          :key="`${connection.sourceId}-${connection.targetId}`"
          type="button"
          @click="
            connection.hasVolumeEffect
              ? emit('remove-effect', connection.sourceId, connection.targetId)
              : emit('add-effect', connection.sourceId, connection.targetId)
          "
        >
          {{ connection.hasVolumeEffect ? "Remove" : "Add" }} volume control → {{ connection.targetLabel }}
        </button>
      </template>
    </template>
    <template v-else-if="target.kind === 'edge'">
      <button v-if="!target.hasVolumeEffect" type="button" @click="emit('add-effect', target.sourceId, target.targetId)">
        Add volume control
      </button>
      <button v-else type="button" @click="emit('remove-effect', target.sourceId, target.targetId)">
        Remove volume control
      </button>
    </template>
    <template v-else>
      <p class="routing-graph-context-menu-label">Add node</p>
      <button type="button" @click="emit('add-node', 'output')">+ Virtual Output</button>
      <button type="button" @click="emit('add-node', 'input')">+ Virtual Input</button>
    </template>
  </div>
</template>
