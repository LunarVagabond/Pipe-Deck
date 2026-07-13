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
  ungroup: (groupId: string) => void;
  labelForEntity: (entityId: string) => string;
}

export const routingGraphActionsKey: InjectionKey<RoutingGraphActions> =
  Symbol("routingGraphActions");
