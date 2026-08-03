<script setup lang="ts">
import { inject } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { routingGraphActionsKey } from "../composables/routingGraphContext";

const props = defineProps<{
  nodeId: string;
  volumePercent: number;
  muted: boolean;
  members: { id: string; label: string; portIndex: number }[];
}>();

const actions = inject(routingGraphActionsKey, null);

async function onVolumeChange(event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  await invoke("update_processing_node_volume", { nodeId: props.nodeId, volumePercent: value, muted: props.muted });
}

async function onToggleMute() {
  await invoke("update_processing_node_volume", {
    nodeId: props.nodeId,
    volumePercent: props.volumePercent,
    muted: !props.muted,
  });
}

// Issue #80/PD-035 revision: a Group node behaves like a terminal output —
// its members render as an inline name list here, never as real graph
// edges to the actual device nodes (see collectEdges.ts) — so removing one
// is a disconnect on this node's own growable output side, the same
// primitive drag-disconnecting a Fan-out's output already uses.
async function onRemoveMember(portIndex: number) {
  await invoke("disconnect_processing_node_port", { nodeId: props.nodeId, direction: "output", portIndex });
}
</script>

<template>
  <div class="routing-graph-node-effects nodrag">
    <div class="routing-graph-node-effect-row routing-graph-node-effect-row--pinned">
      <button
        type="button"
        class="routing-graph-node-mute"
        :class="{ active: muted }"
        :aria-label="muted ? 'Unmute' : 'Mute'"
        @click="onToggleMute"
      >
        {{ muted ? "🔇" : "🔊" }}
      </button>
      <input
        type="range"
        class="routing-graph-node-volume"
        min="0"
        max="100"
        :value="volumePercent"
        aria-label="Group volume"
        @change="onVolumeChange"
      />
      <span class="routing-graph-node-volume-label">{{ volumePercent }}%</span>
    </div>

    <ul v-if="actions?.isGroupExpanded(nodeId)" class="routing-graph-group-members">
      <li
        v-for="member in members"
        :key="member.id"
        class="routing-graph-group-member"
        @mouseenter="actions?.setHighlightedNode(member.id)"
        @mouseleave="actions?.setHighlightedNode(null)"
      >
        <span class="routing-graph-group-member-label">{{ member.label }}</span>
        <button
          type="button"
          class="icon-btn routing-graph-group-member-remove"
          title="Remove from group"
          aria-label="Remove from group"
          @click="onRemoveMember(member.portIndex)"
        >
          ✕
        </button>
      </li>
      <li v-if="members.length === 0" class="routing-graph-group-member routing-graph-group-member--empty">
        No members
      </li>
    </ul>
  </div>
</template>
