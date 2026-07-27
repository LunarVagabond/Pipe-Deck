<script setup lang="ts">
import { computed, inject, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useApplyResult } from "../stores/notices";
import { routingGraphActionsKey } from "../composables/routingGraphContext";
import { processingNodeNodeId } from "./routing-graph/nodeIds";

const props = defineProps<{
  nodeId: string;
  ceilingDb: number;
  floorDb: number;
  symmetric: boolean;
  bypassed: boolean;
}>();

const { handleApplyResult } = useApplyResult();
const actions = inject(routingGraphActionsKey, null);
const graphNodeId = computed(() => processingNodeNodeId(props.nodeId));
const isIsolated = computed(() => actions?.isEffectIsolated(graphNodeId.value) ?? false);

function onToggleIsolate() {
  void actions?.isolateEffectNode(graphNodeId.value);
}

/** Keeps a dragged control's value on screen until the next graph push
 * confirms it — same reasoning as `RoutingGraphNodeDelay.vue`'s `pending`
 * ref: the live-apply push can race the node's own readiness right after
 * creation, and without local state here the slider would visibly snap back
 * to the last server-echoed value for that entire window. */
const pending = ref<{ ceilingDb?: number; floorDb?: number }>({});

const displayCeiling = computed(() => pending.value.ceilingDb ?? props.ceilingDb);
const displayFloor = computed(() => pending.value.floorDb ?? props.floorDb);

async function pushLimiterParams(ceilingDb: number, floorDb: number, symmetric: boolean) {
  const response = await invoke<{ success: boolean; message?: string }>("update_processing_node_limiter_params", {
    nodeId: props.nodeId,
    ceilingDb,
    floorDb,
    symmetric,
  });
  if (!response.success) {
    handleApplyResult(response, "");
  }
}

function onCeilingInput(event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  pending.value.ceilingDb = value;
  // While symmetric, the floor slider is hidden and always mirrors the
  // ceiling — reflect that mirroring in the on-screen value too, not just
  // in the eventual @change push, so the (invisible) floor never visibly
  // lags behind mid-drag.
  if (props.symmetric) {
    pending.value.floorDb = value;
  }
}

/** Dragging Ceiling while symmetric also moves Floor to match (a single
 * locked control from the user's perspective); while unlocked, only Ceiling
 * moves and Floor is sent unchanged. */
async function onCeilingChange(event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  pending.value.ceilingDb = value;
  const floorDb = props.symmetric ? value : props.floorDb;
  if (props.symmetric) pending.value.floorDb = value;
  await pushLimiterParams(value, floorDb, props.symmetric);
}

function onFloorInput(event: Event) {
  pending.value.floorDb = Number((event.target as HTMLInputElement).value);
}

/** Only reachable while unlocked (the Floor slider is hidden while
 * symmetric) — moves Floor independently of Ceiling. */
async function onFloorChange(event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  pending.value.floorDb = value;
  await pushLimiterParams(props.ceilingDb, value, false);
}

/** Toggling to symmetric snaps Floor to whatever Ceiling currently is (the
 * "symmetric recovery" behavior) — an unlocked ceiling/floor pair that
 * happened to differ doesn't silently average or keep the stale floor
 * value once re-locked. Toggling to asymmetric just unlocks; values don't
 * change (they're already equal, since they were locked). */
async function onToggleSymmetric() {
  if (props.symmetric) {
    await pushLimiterParams(props.ceilingDb, props.floorDb, false);
    return;
  }
  pending.value.floorDb = props.ceilingDb;
  await pushLimiterParams(props.ceilingDb, props.ceilingDb, true);
}

/** Resets Ceiling/Floor to full scale (no clamp) and re-locks symmetric —
 * same command every slider already uses. */
async function onReset() {
  pending.value = {};
  await pushLimiterParams(0, 0, true);
}

/** Keeps every connection exactly as wired — only whether the signal comes
 * through processed or not changes. */
async function onToggleBypass() {
  const response = await invoke<{ success: boolean; message?: string }>("set_processing_node_bypassed", {
    nodeId: props.nodeId,
    bypassed: !props.bypassed,
  });
  if (!response.success) {
    handleApplyResult(response, "");
  }
}
</script>

<template>
  <div class="routing-graph-node-limiter nodrag" :class="{ 'is-bypassed': bypassed }">
    <div class="routing-graph-node-limiter-actions">
      <button
        type="button"
        class="routing-graph-node-limiter-bypass"
        :class="{ active: bypassed }"
        :aria-pressed="bypassed"
        :title="bypassed ? 'Bypassed — passing through unprocessed' : 'Bypass — keep wiring, skip processing'"
        @click="onToggleBypass"
      >
        {{ bypassed ? "Bypassed" : "Bypass" }}
      </button>
      <button
        type="button"
        class="routing-graph-node-limiter-isolate"
        :class="{ active: isIsolated }"
        :aria-pressed="isIsolated"
        :title="isIsolated ? 'Isolated — click to restore other effects' : 'Isolate — bypass every other effect in this chain'"
        @click="onToggleIsolate"
      >
        {{ isIsolated ? "Isolated" : "Isolate" }}
      </button>
      <button
        type="button"
        class="routing-graph-node-limiter-symmetric"
        :class="{ active: symmetric }"
        :aria-pressed="symmetric"
        :title="symmetric ? 'Symmetric — Floor always matches Ceiling. Click to unlock and set them independently.' : 'Asymmetric — Ceiling and Floor are independent. Click to re-lock (snaps Floor to Ceiling).'"
        @click="onToggleSymmetric"
      >
        {{ symmetric ? "Symmetric" : "Asymmetric" }}
      </button>
      <button
        type="button"
        class="routing-graph-node-limiter-reset"
        title="Reset Ceiling/Floor to full scale (no clamp) and re-lock symmetric"
        aria-label="Reset to defaults"
        @click="onReset"
      >
        ↺
      </button>
      <span
        class="routing-graph-node-dsp-warning"
        title="Hard brick-wall clamp — no envelope smoothing or lookahead, unlike a real limiter. Aggressive settings will sound harsh/distorted. Real dynamics processing is tracked in issue #86."
        aria-label="Hard clamp only, no smoothing — aggressive settings will sound harsh — see issue #86"
      >
        ⚠
      </span>
    </div>
    <div class="routing-graph-node-limiter-row">
      <span class="routing-graph-node-limiter-label">Ceiling</span>
      <input
        type="range"
        class="routing-graph-node-limiter-slider"
        min="-24"
        max="0"
        :value="displayCeiling"
        :disabled="bypassed"
        aria-label="Ceiling"
        @input="onCeilingInput"
        @change="onCeilingChange"
      />
      <span class="routing-graph-node-limiter-value">{{ displayCeiling }}dB</span>
    </div>
    <div v-if="!symmetric" class="routing-graph-node-limiter-row">
      <span class="routing-graph-node-limiter-label">Floor</span>
      <input
        type="range"
        class="routing-graph-node-limiter-slider"
        min="-24"
        max="0"
        :value="displayFloor"
        :disabled="bypassed"
        aria-label="Floor"
        @input="onFloorInput"
        @change="onFloorChange"
      />
      <span class="routing-graph-node-limiter-value">{{ displayFloor }}dB</span>
    </div>
  </div>
</template>
