import type { InjectionKey } from "vue";
import type { RoutingGraphHandle } from "../components/routing-graph/buildGraph";

export interface RoutingGraphNodeMenuTarget {
  kind: "node";
  x: number;
  y: number;
  label: string;
  /** Absent for stream nodes — streams have no PipeWire-side alias/rename
   * target, only a RuntimeGraph `entityId`. */
  systemName?: string;
  /** The underlying RuntimeGraph device/stream id — always present, unlike
   * `deviceId` below which is scoped to effects-capable device nodes.
   * "Copy ID" copies `systemName` when present (the real PipeWire node
   * name) and only falls back to this internal id for streams, which have
   * no `systemName`. */
  entityId: string;
  editable: boolean;
  deletable: boolean;
  /** Present only for a device node (not a stream) that's effects-capable —
   * gates the "+ Effect" menu entry. See `core/engine/effects_ops.rs`'s
   * `is_pipe_deck_device` guard for why streams/physical devices never get
   * this even if `supportsEffects` looked true on the graph node itself. */
  deviceId?: string;
  supportsEffects?: boolean;
  existingStageKinds?: string[];
}

export interface RoutingGraphPaneMenuTarget {
  kind: "pane";
  x: number;
  y: number;
}

/** Opened instead of `RoutingGraphNodeMenuTarget` when 2+ output-direction
 * device nodes are selected at the moment of a right-click (issue #80) —
 * offers "Group Selected Outputs" rather than the normal single-node menu. */
export interface RoutingGraphMultiNodeMenuTarget {
  kind: "multi-node";
  x: number;
  y: number;
  memberDeviceIds: string[];
  memberLabels: string[];
}

export type RoutingGraphMenuTarget =
  | RoutingGraphNodeMenuTarget
  | RoutingGraphPaneMenuTarget
  | RoutingGraphMultiNodeMenuTarget;

export interface RoutingGraphActions {
  openMenu: (target: RoutingGraphMenuTarget) => void;
  closeMenu: () => void;
  renameDevice: (systemName: string, currentLabel: string, alias?: string) => void | Promise<void>;
  deleteDevice: (systemName: string, label: string) => void;
  /** PD-032: a processing node (Mixer/Fan-out/EQ/stub) has no PipeWire
   * device-alias identity — deleting it goes through `remove_processing_node`
   * by its RuntimeGraph id, not `deleteDevice`'s system_name-keyed path. */
  deleteProcessingNode: (nodeId: string, label: string) => void;
  renameGroup: (groupId: string, label: string) => void;
  setGroupColor: (groupId: string, color: string) => void;
  ungroup: (groupId: string) => void;
  labelForEntity: (entityId: string) => string;
  /** Keyboard equivalent of dragging a wire end off a port: disconnects the
   * one link `handle` represents. No-op if `handle` isn't a live connection. */
  disconnectPort: (nodeId: string, handle: RoutingGraphHandle) => void | Promise<void>;
  /** PD-025: adds a 5-Band EQ stage to `deviceId` and applies immediately —
   * no separate confirm step. */
  addEffectStage: (deviceId: string) => void | Promise<void>;
  /** Recovers a node that's been dragged (or auto-laid-out) off-canvas by
   * relocating it to the screen point `x`/`y` — typically the pane
   * right-click point that opened the "Bring node here" menu (issue #142). */
  bringNodeHere: (nodeId: string, x: number, y: number) => void;
  /** Isolate (#222): bypasses every other effect-capable processing node
   * (Eq5Band today) in `nodeId`'s connected signal chain, restoring each to
   * its exact prior bypassed state when un-isolated. Never touches Mixer/
   * Fan-out nodes, devices, or streams. */
  isolateEffectNode: (nodeId: string) => void | Promise<void>;
  isEffectIsolated: (nodeId: string) => boolean;
  /** Issue #80/PD-035: shows/hides a Group node's inline member-name list
   * (`RoutingGraphNodeGroup.vue`) — collapsed by default. Purely canvas
   * display state, no backend effect. A Group never draws real edges to its
   * members regardless of this state (see `collectEdges.ts`). */
  toggleGroupExpansion: (nodeId: string) => void;
  isGroupExpanded: (nodeId: string) => boolean;
  /** Issue #80/PD-035: hovering a member row in a Group node's expanded list
   * highlights that member's real node elsewhere on the canvas, so a user
   * can locate it without a persistent edge cluttering the graph. `null`
   * clears the highlight. Purely canvas display state. */
  setHighlightedNode: (entityId: string | null) => void;
  isNodeHighlighted: (entityId: string) => boolean;
}

export const routingGraphActionsKey: InjectionKey<RoutingGraphActions> =
  Symbol("routingGraphActions");
