import type { InjectionKey } from "vue";
import type { RoutingGraphHandle } from "../components/routing-graph/buildGraph";

export interface RoutingGraphNodeMenuTarget {
  kind: "node";
  x: number;
  y: number;
  label: string;
  systemName: string;
  editable: boolean;
  deletable: boolean;
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
}

export const routingGraphActionsKey: InjectionKey<RoutingGraphActions> =
  Symbol("routingGraphActions");
