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
   * `deviceId` below which is scoped to effects-capable device nodes. This
   * is what "Copy ID" copies. */
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

export type RoutingGraphMenuTarget = RoutingGraphNodeMenuTarget | RoutingGraphPaneMenuTarget;

export interface RoutingGraphActions {
  openMenu: (target: RoutingGraphMenuTarget) => void;
  closeMenu: () => void;
  renameDevice: (systemName: string, currentLabel: string, alias?: string) => void | Promise<void>;
  deleteDevice: (systemName: string, label: string) => void;
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
}

export const routingGraphActionsKey: InjectionKey<RoutingGraphActions> =
  Symbol("routingGraphActions");
