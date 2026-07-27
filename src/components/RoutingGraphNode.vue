<script setup lang="ts">
import { computed, inject, ref } from "vue";
import { Handle, Position, useNodeId } from "@vue-flow/core";
import NodeCardHeader from "./NodeCardHeader.vue";
import NodeTypeIcon from "./NodeTypeIcon.vue";
import RoutingGraphNodeEffects from "./RoutingGraphNodeEffects.vue";
import RoutingGraphNodeMixer from "./RoutingGraphNodeMixer.vue";
import RoutingGraphNodeEq5Band from "./RoutingGraphNodeEq5Band.vue";
import RoutingGraphNodeDelay from "./RoutingGraphNodeDelay.vue";
import RoutingGraphNodeLimiter from "./RoutingGraphNodeLimiter.vue";
import RoutingGraphNodeHpf from "./RoutingGraphNodeHpf.vue";
import RoutingGraphNodeReverb from "./RoutingGraphNodeReverb.vue";
import RoutingGraphNodeWidener from "./RoutingGraphNodeWidener.vue";
import RoutingGraphNodeFanOut from "./RoutingGraphNodeFanOut.vue";
import type { RoutingGraphHandle, RoutingGraphNodeData } from "./routing-graph/buildGraph";
import { useMixerControls } from "../composables/useMixerControls";
import { useEffectChain } from "../composables/useEffectChain";
import { routingGraphActionsKey } from "../composables/routingGraphContext";

const props = defineProps<{
  data: RoutingGraphNodeData;
}>();

const actions = inject(routingGraphActionsKey, null);
const nodeId = useNodeId();
const { pendingVolumes, clampVolume, scheduleChannelVolume, toggleChannelMute } =
  useMixerControls();
const { chainFor } = useEffectChain();

function existingStageKindsFor(deviceId: string): string[] {
  return chainFor(deviceId).stages.map((stage) => stage.kind);
}

/** Tri-state so "effects live" and "effects bypassed" read as visually and
 * textually distinct at a glance, not just a width change (see #153). */
const effectsState = computed<"none" | "live" | "bypassed">(() => {
  if (props.data.channelType !== "device") return "none";
  const chain = chainFor(props.data.entityId);
  if (chain.stages.length === 0) return "none";
  return chain.bypassed ? "bypassed" : "live";
});

const hasEffectStages = computed(() => effectsState.value !== "none");

// `nodeClass` is CSS-class-shaped (e.g. "processing-node processing-node--mixer",
// for the multi-class `:class` binding below) — not a single `NodeTypeIcon`
// kind, so a processing node always fell through to the generic dot
// fallback. Icon lookup uses the node's own `kind.kind` slug instead for
// that case; every other node kind's `nodeClass` was already a single word
// matching `NodeTypeIcon`'s expected values.
const iconKind = computed(() => props.data.processingNodeKind?.kind ?? props.data.nodeClass);

const effectsBadgeTitle = computed(() =>
  effectsState.value === "live" ? "Effects live" : effectsState.value === "bypassed" ? "Effects bypassed" : "",
);

/** DSP-backed processing node kinds (Eq5Band/Delay/Limiter/Hpf) each expose a
 * `reset()` method (see their own `defineExpose`) — Reset and the DSP
 * warning both render once, here in the node's header (top-right, next to
 * Delete), rather than duplicated per-kind in each child's own template. */
const eq5bandRef = ref<{ reset: () => void | Promise<void> } | null>(null);
const delayRef = ref<{ reset: () => void | Promise<void> } | null>(null);
const limiterRef = ref<{ reset: () => void | Promise<void> } | null>(null);
const hpfRef = ref<{ reset: () => void | Promise<void> } | null>(null);
const reverbRef = ref<{ reset: () => void | Promise<void> } | null>(null);
const widenerRef = ref<{ reset: () => void | Promise<void> } | null>(null);

const DSP_WARNING_TEXT: Partial<Record<string, string>> = {
  eq5band:
    "Boosting bands can push the signal above full scale — any resulting clipping happens at the output (hardware/sink), not here, and sounds like harsh digital distortion. Real dynamics processing (to catch this smoothly) is tracked in issue #86.",
  delay:
    "High Feedback can build up into a resonant loop that eventually clips — a real limiter/compressor stage after this would catch it smoothly; tracked in issue #86.",
  limiter:
    "Hard brick-wall clamp — no envelope smoothing or lookahead, unlike a real limiter. Aggressive settings will sound harsh/distorted. Real dynamics processing is tracked in issue #86.",
  hpf: "A high Resonance value creates a sharp peak right at the cutoff frequency, which can ring or self-oscillate on some material. Lower it if the filtered signal sounds harsh or whistling.",
  widener:
    "High Width settings can push the side signal loud enough to clip when summed back to mono (e.g. on a phone speaker or older Bluetooth receiver). Real dynamics processing (to catch this smoothly) is tracked in issue #86.",
};
const dspWarningText = computed(() => DSP_WARNING_TEXT[props.data.processingNodeKind?.kind ?? ""]);

function onResetClick() {
  void (eq5bandRef.value ?? delayRef.value ?? limiterRef.value ?? hpfRef.value ?? reverbRef.value ?? widenerRef.value)?.reset();
}

const inHandles = computed(() => props.data.handles.filter((handle) => handle.position === "left"));
const outHandles = computed(() =>
  props.data.handles.filter((handle) => handle.position === "right"),
);

function portTitle(handle: RoutingGraphHandle): string {
  if (handle.empty) {
    return "Not connected — drag here to connect";
  }
  if (handle.connectedId) {
    return actions?.labelForEntity(handle.connectedId) ?? "Connected";
  }
  return "";
}

/** Screen-reader label for a port: what it is, and what (if anything) it's
 * wired to today — the sighted view conveys the same via `portTitle`'s
 * hover tooltip plus the port's filled/empty styling. */
function handleAriaLabel(handle: RoutingGraphHandle): string {
  const direction = handle.type === "source" ? "output" : "input";
  if (handle.empty) {
    return `${props.data.label} ${direction} port, not connected`;
  }
  const other = handle.connectedId ? actions?.labelForEntity(handle.connectedId) : undefined;
  return `${props.data.label} ${direction} port, connected to ${other ?? "another device"}`;
}

/** Enter/Space triggers the same click Vue Flow's own click-to-connect
 * handling already listens for (see useHandle in @vue-flow/core) — reusing
 * that state machine instead of re-implementing connect validation here.
 * Delete/Backspace on an occupied port is the keyboard equivalent of
 * dragging a wire end off to disconnect it. */
function onHandleKeydown(event: KeyboardEvent, handle: RoutingGraphHandle) {
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    (event.currentTarget as HTMLElement).click();
    return;
  }
  if (event.key === "Delete" || event.key === "Backspace") {
    if (handle.empty || !handle.connectedId || !nodeId) return;
    event.preventDefault();
    void actions?.disconnectPort(nodeId, handle);
  }
}

function onContextMenu(event: MouseEvent) {
  event.preventDefault();
  event.stopPropagation();
  const deviceId = props.data.channelType === "device" ? props.data.entityId : undefined;
  actions?.openMenu({
    kind: "node",
    x: event.clientX,
    y: event.clientY,
    label: props.data.label,
    systemName: props.data.systemName,
    entityId: props.data.entityId,
    editable: Boolean(props.data.editable),
    deletable: Boolean(props.data.deletable),
    deviceId,
    supportsEffects: props.data.supportsEffects,
    existingStageKinds: deviceId ? existingStageKindsFor(deviceId) : [],
  });
}

function onRename(alias: string) {
  if (!props.data.systemName) return;
  actions?.renameDevice(props.data.systemName, props.data.label, alias);
}

function onDelete() {
  // PD-032: a processing node has no device-alias identity — it goes
  // through remove_processing_node by RuntimeGraph id, not deleteDevice's
  // system_name-keyed path (which would silently hit the wrong backend
  // command for it — see NodeCardHeader's inline delete button, the one
  // entry point that isn't gated through the right-click context menu's
  // own pipe-deck-proc- routing check).
  if (props.data.processingNodeKind) {
    actions?.deleteProcessingNode(props.data.entityId, props.data.label);
    return;
  }
  if (!props.data.systemName) return;
  actions?.deleteDevice(props.data.systemName, props.data.label);
}

// Plain volume control for hardware (physical) devices — no effects list,
// no drag handle, just the bare slider that's been here since before any of
// the effects work (see RoutingGraphNodeEffects.vue for the virtual/stream case).
const displayVolume = computed(() => {
  if (!props.data.channelType) return 0;
  return pendingVolumes.value[props.data.entityId] ?? props.data.volumePercent ?? 0;
});

function onVolumeInput(event: Event) {
  if (!props.data.channelType) return;
  const percent = Number((event.target as HTMLInputElement).value);
  scheduleChannelVolume(props.data.channelType, props.data.entityId, clampVolume(percent));
}

function onToggleMute() {
  if (!props.data.channelType) return;
  void toggleChannelMute(props.data.channelType, props.data.entityId, Boolean(props.data.muted));
}
</script>

<template>
  <div
    class="routing-graph-node nopan"
    :class="[
      data.nodeClass,
      {
        'routing-graph-node--has-effects': hasEffectStages,
        'routing-graph-node--effects-bypassed': effectsState === 'bypassed',
      },
    ]"
    @contextmenu="onContextMenu"
  >
    <div v-if="inHandles.length" class="routing-graph-node-ports routing-graph-node-ports--in">
      <div
        v-for="handle in inHandles"
        :key="handle.id"
        class="routing-graph-port-row"
        :class="{ 'is-empty': handle.empty }"
        :title="portTitle(handle)"
      >
        <Handle
          :id="handle.id"
          type="target"
          :position="Position.Left"
          class="routing-graph-handle"
          :class="{ 'is-empty': handle.empty }"
          tabindex="0"
          role="button"
          :aria-label="handleAriaLabel(handle)"
          @keydown="(event) => onHandleKeydown(event, handle)"
        />
      </div>
    </div>

    <div class="routing-graph-node-main">
      <div class="routing-graph-node-body">
        <span
          v-if="data.accent"
          class="routing-graph-node-swatch"
          :style="{ background: data.accent }"
        />
        <NodeTypeIcon :kind="iconKind" class="routing-graph-node-icon" />
        <span
          v-if="effectsState !== 'none'"
          class="routing-graph-node-effects-badge"
          :class="`routing-graph-node-effects-badge--${effectsState}`"
          :title="effectsBadgeTitle"
          :aria-label="effectsBadgeTitle"
        />
        <span
          v-if="data.routeWarning"
          class="routing-graph-node-warning-badge"
          :class="`routing-graph-node-warning-badge--${data.routeWarning}`"
          :title="data.routeWarningTitle"
          :aria-label="data.routeWarningTitle"
        />
        <div class="routing-graph-node-copy">
          <NodeCardHeader
            v-if="data.systemName"
            :label="data.label"
            :editable="data.editable"
            :deletable="data.deletable"
            layout="inline"
            @save="onRename"
            @delete="onDelete"
          >
            <template v-if="dspWarningText" #toolbar-extra>
              <button
                type="button"
                class="icon-btn routing-graph-node-header-reset"
                title="Reset to defaults"
                aria-label="Reset to defaults"
                @click="onResetClick"
              >
                ↺
              </button>
              <span
                class="routing-graph-node-dsp-warning"
                :title="dspWarningText"
                :aria-label="dspWarningText"
              >
                ⚠
              </span>
            </template>
          </NodeCardHeader>
          <strong v-else>{{ data.label }}</strong>
          <span class="routing-graph-node-sub">{{ data.subtitle }}</span>
        </div>
      </div>

      <!-- `handlesForProcessingNode` builds audio-in handles in the same
           order as `ProcessingNode.inputs`, so filtered position === real
           port index; see nodePorts.ts. -->
      <RoutingGraphNodeMixer
        v-if="data.processingNodeKind?.kind === 'mixer'"
        :node-id="data.entityId"
        :input-gains-percent="data.processingNodeKind.input_gains_percent"
        :inputs="inHandles.filter((h) => !h.empty).map((h, i) => ({ index: i, connectedId: h.connectedId }))"
      />
      <RoutingGraphNodeEq5Band
        v-else-if="data.processingNodeKind?.kind === 'eq5band'"
        ref="eq5bandRef"
        :node-id="data.entityId"
        :eq-sub="data.processingNodeKind.eq_sub"
        :eq-bass="data.processingNodeKind.eq_bass"
        :eq-mid="data.processingNodeKind.eq_mid"
        :eq-treble="data.processingNodeKind.eq_treble"
        :eq-air="data.processingNodeKind.eq_air"
        :output-gain="data.processingNodeKind.output_gain"
        :bypassed="data.processingNodeBypassed ?? false"
      />
      <RoutingGraphNodeDelay
        v-else-if="data.processingNodeKind?.kind === 'delay'"
        ref="delayRef"
        :node-id="data.entityId"
        :delay-ms="data.processingNodeKind.delay_ms"
        :feedback-percent="data.processingNodeKind.feedback_percent"
        :feedforward-percent="data.processingNodeKind.feedforward_percent"
        :bypassed="data.processingNodeBypassed ?? false"
      />
      <RoutingGraphNodeLimiter
        v-else-if="data.processingNodeKind?.kind === 'limiter'"
        ref="limiterRef"
        :node-id="data.entityId"
        :ceiling-db="data.processingNodeKind.ceiling_db"
        :floor-db="data.processingNodeKind.floor_db"
        :symmetric="data.processingNodeKind.symmetric"
        :bypassed="data.processingNodeBypassed ?? false"
      />
      <RoutingGraphNodeHpf
        v-else-if="data.processingNodeKind?.kind === 'hpf'"
        ref="hpfRef"
        :node-id="data.entityId"
        :freq-hz="data.processingNodeKind.freq_hz"
        :resonance-x10="data.processingNodeKind.resonance_x10"
        :bypassed="data.processingNodeBypassed ?? false"
      />
      <RoutingGraphNodeReverb
        v-else-if="data.processingNodeKind?.kind === 'reverb'"
        ref="reverbRef"
        :node-id="data.entityId"
        :mix-percent="data.processingNodeKind.mix_percent"
        :bypassed="data.processingNodeBypassed ?? false"
      />
      <RoutingGraphNodeWidener
        v-else-if="data.processingNodeKind?.kind === 'widener'"
        ref="widenerRef"
        :node-id="data.entityId"
        :width-percent="data.processingNodeKind.width_percent"
        :bypassed="data.processingNodeBypassed ?? false"
      />
      <RoutingGraphNodeFanOut
        v-else-if="data.processingNodeKind?.kind === 'fan_out'"
        :node-id="data.entityId"
        :volume-percent="data.processingNodeKind.volume_percent"
        :muted="data.processingNodeKind.muted"
      />
      <p v-else-if="data.processingNodeKind?.kind === 'stub'" class="routing-graph-node-stub-label">
        Not implemented yet
      </p>
      <RoutingGraphNodeEffects
        v-else-if="data.channelType && data.supportsEffects"
        :channel-type="data.channelType"
        :entity-id="data.entityId"
        :label="data.label"
        :volume-percent="data.volumePercent"
        :muted="data.muted"
        :device-id="data.channelType === 'device' ? data.entityId : undefined"
      />
      <div v-else-if="data.channelType" class="routing-graph-node-mixer nodrag">
        <button
          type="button"
          class="routing-graph-node-mute"
          :class="{ active: data.muted }"
          :aria-label="data.muted ? 'Unmute' : 'Mute'"
          @click="onToggleMute"
        >
          {{ data.muted ? "🔇" : "🔊" }}
        </button>
        <input
          type="range"
          class="routing-graph-node-volume"
          min="0"
          max="100"
          :value="displayVolume"
          :aria-label="`${data.label} volume`"
          @input="onVolumeInput"
        />
        <span class="routing-graph-node-volume-label">{{ displayVolume }}%</span>
      </div>
    </div>

    <div v-if="outHandles.length" class="routing-graph-node-ports routing-graph-node-ports--out">
      <div
        v-for="handle in outHandles"
        :key="handle.id"
        class="routing-graph-port-row"
        :class="{ 'is-empty': handle.empty }"
        :title="portTitle(handle)"
      >
        <Handle
          :id="handle.id"
          type="source"
          :position="Position.Right"
          class="routing-graph-handle"
          :class="{ 'is-empty': handle.empty }"
          tabindex="0"
          role="button"
          :aria-label="handleAriaLabel(handle)"
          @keydown="(event) => onHandleKeydown(event, handle)"
        />
      </div>
    </div>
  </div>
</template>
