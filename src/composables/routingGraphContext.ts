import type { InjectionKey } from "vue";

export interface RoutingGraphNodeMenuTarget {
  kind: "node";
  x: number;
  y: number;
  label: string;
  systemName: string;
  editable: boolean;
  deletable: boolean;
}

export interface RoutingGraphEdgeMenuTarget {
  kind: "edge";
  x: number;
  y: number;
  edgeId: string;
  hasReroutes: boolean;
}

export type RoutingGraphMenuTarget = RoutingGraphNodeMenuTarget | RoutingGraphEdgeMenuTarget;

export interface RoutingGraphActions {
  openMenu: (target: RoutingGraphMenuTarget) => void;
  closeMenu: () => void;
  renameDevice: (systemName: string, currentLabel: string, alias?: string) => void;
  deleteDevice: (systemName: string, label: string) => void;
  clearEdgeReroutes: (edgeId: string) => void;
  clearAllReroutes: () => void;
}

export const routingGraphActionsKey: InjectionKey<RoutingGraphActions> =
  Symbol("routingGraphActions");
