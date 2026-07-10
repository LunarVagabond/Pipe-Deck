import type { InjectionKey } from "vue";
import type { AppView } from "../types/graph";

export const navigateKey: InjectionKey<(view: AppView) => void> = Symbol("navigate");
