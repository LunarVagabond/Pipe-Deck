<script setup lang="ts">
import { useShortcutsModal } from "../stores/shortcutsModal";
import { LEGEND_ENTRIES } from "./routing-graph/portTypes";

const { shortcutsModalOpen, closeShortcutsModal } = useShortcutsModal();
const legend = LEGEND_ENTRIES;

interface ShortcutEntry {
  keys: string[];
  description: string;
}

interface ShortcutGroup {
  title: string;
  shortcuts: ShortcutEntry[];
}

// Kept in sync by hand with the keydown handlers that actually implement
// each shortcut — see RoutingGraph.vue (group/cancel-connection),
// RoutingGraphNode.vue (per-port connect/disconnect), and the dialog
// components (NodeCardHeader.vue/PromptDialog.vue/RoutingGraphGroupNode.vue)
// for confirm/cancel on inline rename fields.
const groups: ShortcutGroup[] = [
  {
    title: "Global",
    shortcuts: [
      { keys: ["?"], description: "Open this shortcuts reference" },
      { keys: ["Esc"], description: "Close a dialog or modal" },
    ],
  },
  {
    title: "Routing graph",
    shortcuts: [
      {
        keys: ["Drag"],
        description:
          "From an output port to an input port to connect — a new empty slot appears after each connection. Drag a wire off its slot to disconnect.",
      },
      { keys: ["Tab"], description: "Move focus between node ports" },
      { keys: ["Enter", "Space"], description: "Start or complete a connection at the focused port" },
      { keys: ["Delete", "Backspace"], description: "Disconnect the focused port, or delete the selected node/edge" },
      { keys: ["Esc"], description: "Cancel an in-progress connection" },
      { keys: ["G"], description: "Group two or more selected nodes" },
    ],
  },
  {
    title: "Inline rename (groups, dialogs)",
    shortcuts: [
      { keys: ["Enter"], description: "Confirm the new name" },
      { keys: ["Esc"], description: "Cancel and revert" },
    ],
  },
];
</script>

<template>
  <div
    v-if="shortcutsModalOpen"
    class="shortcuts-modal-overlay"
    @click.self="closeShortcutsModal"
  >
    <div class="shortcuts-modal" role="dialog" aria-modal="true" aria-label="Keyboard shortcuts">
      <div class="shortcuts-modal-header">
        <h2>Keyboard shortcuts</h2>
        <button
          type="button"
          class="shortcuts-modal-close"
          aria-label="Close"
          @click="closeShortcutsModal"
        >
          <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
            <path
              d="M6 6l12 12M18 6L6 18"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
            />
          </svg>
        </button>
      </div>
      <div class="shortcuts-modal-body">
        <section v-for="group in groups" :key="group.title" class="shortcuts-group">
          <h3>{{ group.title }}</h3>
          <ul>
            <li v-for="shortcut in group.shortcuts" :key="shortcut.description" class="shortcuts-row">
              <span class="shortcuts-keys">
                <template v-for="(key, index) in shortcut.keys" :key="key">
                  <kbd>{{ key }}</kbd>
                  <span v-if="index < shortcut.keys.length - 1" class="shortcuts-keys-sep">or</span>
                </template>
              </span>
              <span class="shortcuts-description">{{ shortcut.description }}</span>
            </li>
          </ul>
        </section>
        <section class="shortcuts-group">
          <h3>Connection colors</h3>
          <ul>
            <li v-for="entry in legend" :key="entry.key" class="shortcuts-row shortcuts-row--legend">
              <span class="shortcuts-legend-swatch" :style="{ background: entry.color }" />
              <span class="shortcuts-description">{{ entry.label }}</span>
            </li>
          </ul>
        </section>
      </div>
    </div>
  </div>
</template>
