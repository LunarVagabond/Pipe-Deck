import { describe, expect, it } from "vitest";
import { defineComponent, h, inject, provide } from "vue";
import { mount } from "@vue/test-utils";
import { routingGraphActionsKey, type RoutingGraphActions } from "./routingGraphContext";

describe("routingGraphActionsKey", () => {
  it("is a stable symbol across imports", async () => {
    const reimported = await import("./routingGraphContext");

    expect(typeof routingGraphActionsKey).toBe("symbol");
    expect(reimported.routingGraphActionsKey).toBe(routingGraphActionsKey);
  });

  it("round-trips a provided value through inject", () => {
    const actions: RoutingGraphActions = {
      openMenu: () => {},
      closeMenu: () => {},
      renameDevice: () => {},
      deleteDevice: () => {},
      renameGroup: () => {},
      setGroupColor: () => {},
      ungroup: () => {},
      labelForEntity: (entityId) => entityId,
      disconnectPort: () => {},
      addEffectStage: () => {},
      bringNodeHere: () => {},
    };

    let injected: RoutingGraphActions | undefined;
    const Child = defineComponent({
      setup() {
        injected = inject(routingGraphActionsKey);
        return () => h("div");
      },
    });
    const Parent = defineComponent({
      setup() {
        provide(routingGraphActionsKey, actions);
        return () => h(Child);
      },
    });

    mount(Parent);

    expect(injected).toBe(actions);
  });
});
