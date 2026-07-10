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

export type RoutingGraphMenuTarget = RoutingGraphNodeMenuTarget;

export interface RoutingGraphActions {
  openMenu: (target: RoutingGraphMenuTarget) => void;
  closeMenu: () => void;
  renameDevice: (systemName: string, currentLabel: string, alias?: string) => void | Promise<void>;
  deleteDevice: (systemName: string, label: string) => void;
}

export const routingGraphActionsKey: InjectionKey<RoutingGraphActions> =
  Symbol("routingGraphActions");
