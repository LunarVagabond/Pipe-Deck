import type { InjectionKey } from "vue";

/** One of a node's outgoing connections, offered from its context menu as an
 * alternate entry point to adding/removing a per-connection Volume effect
 * (the same action available by right-clicking the edge directly). */
export interface RoutingGraphConnectionMenuEntry {
  sourceId: string;
  targetId: string;
  targetLabel: string;
  hasVolumeEffect: boolean;
}

export interface RoutingGraphNodeMenuTarget {
  kind: "node";
  x: number;
  y: number;
  label: string;
  systemName?: string;
  editable: boolean;
  deletable: boolean;
  connections: RoutingGraphConnectionMenuEntry[];
}

export interface RoutingGraphPaneMenuTarget {
  kind: "pane";
  x: number;
  y: number;
}

export interface RoutingGraphEdgeMenuTarget {
  kind: "edge";
  x: number;
  y: number;
  sourceId: string;
  targetId: string;
  hasVolumeEffect: boolean;
}

export type RoutingGraphMenuTarget =
  | RoutingGraphNodeMenuTarget
  | RoutingGraphPaneMenuTarget
  | RoutingGraphEdgeMenuTarget;

export interface RoutingGraphActions {
  openMenu: (target: RoutingGraphMenuTarget) => void;
  closeMenu: () => void;
  renameDevice: (systemName: string, currentLabel: string, alias?: string) => void | Promise<void>;
  deleteDevice: (systemName: string, label: string) => void;
  renameGroup: (groupId: string, label: string) => void;
  ungroup: (groupId: string) => void;
  labelForEntity: (entityId: string) => string;
  outgoingConnectionsFor: (entityId: string) => RoutingGraphConnectionMenuEntry[];
}

export const routingGraphActionsKey: InjectionKey<RoutingGraphActions> =
  Symbol("routingGraphActions");
