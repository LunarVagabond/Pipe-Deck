import { DOMWrapper, mount, flushPromises } from "@vue/test-utils";
import { ref } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import Rules from "./Rules.vue";
import { makeDevice } from "../test/graphFixtures";
import type { Rule, RuntimeGraph } from "../types/graph";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const confirmMock = vi.hoisted(() => vi.fn().mockResolvedValue(true));
vi.mock("../stores/confirm", () => ({
  useConfirm: () => ({ confirm: confirmMock }),
}));

const consumePendingIdentityMock = vi.hoisted(() => vi.fn().mockReturnValue(null));
vi.mock("../stores/ruleDraft", () => ({
  useRuleDraft: () => ({ consumePendingIdentity: consumePendingIdentityMock }),
}));

const pushNoticeMock = vi.hoisted(() => vi.fn());
vi.mock("../stores/notices", () => ({
  useApplyResult: () => ({
    handleApplyResult: (result: { success: boolean; message?: string }, successMessage: string) => {
      if (result.success) {
        pushNoticeMock("success", successMessage);
        return true;
      }
      pushNoticeMock("error", result.message ?? "Operation failed");
      return false;
    },
  }),
}));

const graph = ref<RuntimeGraph>({ devices: [], streams: [], links: [] });
vi.mock("../stores/runtimeGraph", () => ({
  useRuntimeGraph: () => ({ graph }),
}));

function makeRule(overrides: Partial<Rule> = {}): Rule {
  return {
    id: "rule-1",
    name: "Discord to headset",
    enabled: true,
    priority: 10,
    conditions: [{ type: "identity", value: "discord" }],
    action: { target_system_name: "physical-out-1" },
    safeguards: { fallback_policy: "keep_current" },
    ...overrides,
  };
}

// Rules.vue mounts RuleFormModal, which renders via <Teleport to="body">, so
// modal content lands outside the wrapper's own element tree once open.
// Attaching to the real document body and reading through a DOMWrapper on it
// is what actually reaches it; other assertions can keep using `wrapper`.
let activeWrapper: ReturnType<typeof mount> | undefined;

function mountRules() {
  activeWrapper = mount(Rules, { attachTo: document.body });
  return { wrapper: activeWrapper, body: new DOMWrapper(document.body) };
}

afterEach(() => {
  activeWrapper?.unmount();
  activeWrapper = undefined;
});

beforeEach(() => {
  invokeMock.mockReset();
  confirmMock.mockClear();
  consumePendingIdentityMock.mockReturnValue(null);
  pushNoticeMock.mockClear();
  graph.value = {
    devices: [makeDevice({ id: "dev-1", system_name: "physical-out-1", label: "Speakers", direction: "output" })],
    streams: [],
    links: [],
  };
});

describe("Rules view", () => {
  it("loads and lists rules sorted by descending priority on mount", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_rules") {
        return Promise.resolve([
          makeRule({ id: "low", name: "Low priority", priority: 1 }),
          makeRule({ id: "high", name: "High priority", priority: 99 }),
        ]);
      }
      if (cmd === "simulate_rules") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    const { wrapper } = mountRules();
    await flushPromises();

    const names = wrapper.findAll(".rule-name-meta strong").map((node) => node.text());
    expect(names).toEqual(["High priority", "Low priority"]);
  });

  it("shows an empty state when there are no rules", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_rules") return Promise.resolve([]);
      if (cmd === "simulate_rules") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    const { wrapper } = mountRules();
    await flushPromises();

    expect(wrapper.find(".rules-empty-state").text()).toContain("No authored rules yet.");
  });

  it("filters rules by the search query across name, target, and conditions", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_rules") {
        return Promise.resolve([
          makeRule({ id: "a", name: "Discord rule", conditions: [{ type: "identity", value: "discord" }] }),
          makeRule({ id: "b", name: "Firefox rule", conditions: [{ type: "identity", value: "firefox" }] }),
        ]);
      }
      if (cmd === "simulate_rules") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    const { wrapper } = mountRules();
    await flushPromises();

    await wrapper.find(".rules-search-input").setValue("firefox");

    const names = wrapper.findAll(".rule-name-meta strong").map((node) => node.text());
    expect(names).toEqual(["Firefox rule"]);
  });

  it("shows a no-match empty state distinct from the no-rules-yet state", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_rules") return Promise.resolve([makeRule()]);
      if (cmd === "simulate_rules") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    const { wrapper } = mountRules();
    await flushPromises();

    await wrapper.find(".rules-search-input").setValue("nothing-matches-this");

    expect(wrapper.find(".rules-empty-state").text()).toContain('No rules match "nothing-matches-this"');
  });

  it("counts live simulation matches keyed by matched_rule_key against the rule name", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_rules") return Promise.resolve([makeRule({ name: "Discord rule" })]);
      if (cmd === "simulate_rules") {
        return Promise.resolve([
          {
            stream_id: "s1",
            stream_label: "Discord",
            explanation: { matched_rule_key: "Discord rule", match_reasons: [] },
          },
          {
            stream_id: "s2",
            stream_label: "Other",
            explanation: { matched_rule_key: "Some other rule", match_reasons: [] },
          },
        ]);
      }
      return Promise.resolve(undefined);
    });

    const { wrapper } = mountRules();
    await flushPromises();

    expect(wrapper.find(".rule-match-badge").text()).toBe("Matching 1 now");
  });

  it("deletes a rule after confirmation and refreshes the list", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_rules") return Promise.resolve([makeRule()]);
      if (cmd === "simulate_rules") return Promise.resolve([]);
      if (cmd === "delete_rule") return Promise.resolve({ success: true });
      return Promise.resolve(undefined);
    });

    const { wrapper } = mountRules();
    await flushPromises();

    await wrapper.find(".rule-card-actions button.danger").trigger("click");
    await flushPromises();

    expect(confirmMock).toHaveBeenCalled();
    expect(invokeMock).toHaveBeenCalledWith("delete_rule", { ruleId: "rule-1" });
    expect(invokeMock).toHaveBeenCalledWith("list_rules");
  });

  it("does not delete a rule when the confirmation is declined", async () => {
    confirmMock.mockResolvedValueOnce(false);
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_rules") return Promise.resolve([makeRule()]);
      if (cmd === "simulate_rules") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    const { wrapper } = mountRules();
    await flushPromises();

    await wrapper.find(".rule-card-actions button.danger").trigger("click");
    await flushPromises();

    expect(invokeMock).not.toHaveBeenCalledWith("delete_rule", expect.anything());
  });

  it("opens the create-rule modal defaulted to an output target and saves a new rule", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_rules") return Promise.resolve([]);
      if (cmd === "simulate_rules") return Promise.resolve([]);
      if (cmd === "save_rule") return Promise.resolve({ success: true });
      return Promise.resolve(undefined);
    });

    const { wrapper, body } = mountRules();
    await flushPromises();

    await wrapper.find(".rules-new-btn").trigger("click");
    await flushPromises();

    expect(body.find(".rules-modal-dialog").exists()).toBe(true);
    expect(body.find("#create-rule-title").exists()).toBe(true);

    // The blank default condition must be filled in before save passes
    // validation, same as a real user would have to.
    await body.find(".condition-field-grow input").setValue("firefox");

    await body.find(".rules-modal-actions button.primary").trigger("click");
    await flushPromises();

    expect(invokeMock).toHaveBeenCalledWith("save_rule", expect.objectContaining({
      rule: expect.objectContaining({ name: "New rule" }),
    }));
  });

  it("rejects saving a rule with no conditions left after trimming blanks", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_rules") return Promise.resolve([makeRule({ conditions: [{ type: "identity", value: "" }] })]);
      if (cmd === "simulate_rules") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });

    const { wrapper, body } = mountRules();
    await flushPromises();

    await wrapper.find(".rule-card-actions button:not(.danger)").trigger("click");
    await flushPromises();

    await body.find(".rules-modal-actions button.primary").trigger("click");
    await flushPromises();

    expect(invokeMock).not.toHaveBeenCalledWith("save_rule", expect.anything());
    expect(pushNoticeMock).toHaveBeenCalledWith("error", "Add at least one condition");
  });

  it("swaps priority between adjacent rules when moved up or down", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_rules") {
        return Promise.resolve([
          makeRule({ id: "high", name: "High", priority: 20 }),
          makeRule({ id: "low", name: "Low", priority: 10 }),
        ]);
      }
      if (cmd === "simulate_rules") return Promise.resolve([]);
      if (cmd === "save_rule") return Promise.resolve({ success: true });
      return Promise.resolve(undefined);
    });

    const { wrapper } = mountRules();
    await flushPromises();

    const downButtons = wrapper.findAll(".rule-priority-btn").filter((b) => b.text() === "▼");
    await downButtons[0].trigger("click");
    await flushPromises();

    expect(invokeMock).toHaveBeenCalledWith("save_rule", { rule: expect.objectContaining({ id: "high", priority: 10 }) });
    expect(invokeMock).toHaveBeenCalledWith("save_rule", { rule: expect.objectContaining({ id: "low", priority: 20 }) });
  });

  it("surfaces a 'recently seen' identity and can seed a rule from it", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_rules") return Promise.resolve([]);
      if (cmd === "simulate_rules") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    graph.value = {
      devices: [makeDevice({ id: "dev-1", system_name: "physical-out-1", direction: "output" })],
      streams: [],
      links: [],
      recent_stream_identities: [
        {
          app_name: "Discord",
          executable: "discord",
          direction: "playback",
          is_live: false,
          is_system: false,
          last_seen_secs: Math.floor(Date.now() / 1000) - 30,
        },
      ],
    } as RuntimeGraph;

    const { wrapper, body } = mountRules();
    await flushPromises();

    expect(wrapper.find(".rules-panel-recent").exists()).toBe(true);
    await wrapper.find(".recently-seen-create-btn").trigger("click");
    await flushPromises();

    expect(body.find(".rules-modal-dialog").exists()).toBe(true);
    expect(body.find(".rules-modal-dialog h2").text()).toBe("Create rule");
  });
});
