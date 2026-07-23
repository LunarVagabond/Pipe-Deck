import { DOMWrapper, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import RuleFormModal from "./RuleFormModal.vue";
import type { Device, Rule, Stream } from "../../types/graph";

function makeDevice(overrides: Partial<Device> = {}): Device {
  return {
    id: "dev-1",
    system_name: "physical-out-1",
    label: "Speakers",
    kind: "physical",
    direction: "output",
    volume_percent: 80,
    muted: false,
    ...overrides,
  };
}

function makeStream(overrides: Partial<Stream> = {}): Stream {
  return {
    id: "stream-1",
    app_name: "Firefox",
    executable: "firefox",
    direction: "playback",
    ...overrides,
  };
}

function makeRule(overrides: Partial<Rule> = {}): Rule {
  return {
    id: "rule-1",
    name: "New rule",
    enabled: true,
    priority: 10,
    conditions: [{ type: "identity", value: "" }],
    action: { target_system_name: "physical-out-1" },
    safeguards: { fallback_policy: "keep_current" },
    ...overrides,
  };
}

// RuleFormModal renders its dialog via <Teleport to="body">, so its content
// lands outside the mounted wrapper's own element tree. Attaching to the
// real document body and querying through it (rather than through
// `wrapper`) is what actually reaches it.
let activeWrapper: ReturnType<typeof mount> | undefined;

function mountModal(overrides: Record<string, unknown> = {}) {
  activeWrapper = mount(RuleFormModal, {
    attachTo: document.body,
    props: {
      modelValue: makeRule(),
      open: true,
      isEditing: false,
      devices: [
        makeDevice({ id: "dev-out", system_name: "physical-out-1", direction: "output" }),
        makeDevice({ id: "dev-in", system_name: "physical-in-1", direction: "input", label: "Mic" }),
      ],
      identityStreams: [makeStream()],
      recentIdentityIds: new Set<string>(),
      "onUpdate:modelValue": () => {},
      ...overrides,
    },
  });
  return { wrapper: activeWrapper, body: new DOMWrapper(document.body) };
}

afterEach(() => {
  activeWrapper?.unmount();
  activeWrapper = undefined;
});

describe("RuleFormModal", () => {
  it("does not render the dialog when closed", () => {
    const { body } = mountModal({ open: false });
    expect(body.find(".rules-modal-dialog").exists()).toBe(false);
  });

  it("renders create vs edit copy based on isEditing", () => {
    const create = mountModal({ isEditing: false });
    expect(create.body.find("#create-rule-title").text()).toBe("Create rule");
    create.wrapper.unmount();

    const edit = mountModal({ isEditing: true });
    expect(edit.body.find("#edit-rule-title").text()).toBe("Edit rule");
  });

  it("only lists devices matching the current target kind", () => {
    const { body } = mountModal();

    const options = body.find(".rules-target-section select").findAll("option");
    expect(options.map((option) => option.text())).toContain("Speakers — Hardware Output");
    expect(options.map((option) => option.text()).join(" ")).not.toContain("Mic");
  });

  it("switching target kind to input re-filters the device list", async () => {
    const { body } = mountModal();

    await body.findAll(".target-kind-option")[1].trigger("click");

    const options = body.find(".rules-target-section select").findAll("option");
    expect(options.map((option) => option.text()).join(" ")).toContain("Mic");
  });

  it("renders one RuleConditionEditor per condition and adds a new one on 'Add condition'", async () => {
    const rule = makeRule({
      conditions: [
        { type: "identity", value: "" },
        { type: "executable", value: "" },
      ],
    });
    const { wrapper, body } = mountModal({ modelValue: rule });

    expect(wrapper.findAllComponents({ name: "RuleConditionEditor" })).toHaveLength(2);

    await body.find(".rule-conditions-editor-header button").trigger("click");
    expect(rule.conditions).toHaveLength(3);
  });

  it("emits save and cancel", async () => {
    const { wrapper, body } = mountModal();

    await body.find(".rules-modal-actions button.primary").trigger("click");
    expect(wrapper.emitted("save")).toHaveLength(1);

    await body.find(".rules-modal-close").trigger("click");
    expect(wrapper.emitted("cancel")).toHaveLength(1);
  });

  it("shows the identity reference table with live and recent rows distinguished", () => {
    const recentStream = makeStream({ id: "stream-recent", app_name: "Discord" });
    const { body } = mountModal({
      identityStreams: [makeStream(), recentStream],
      recentIdentityIds: new Set(["stream-recent"]),
    });

    const rows = body.findAll(".identity-reference-table tbody tr");
    expect(rows).toHaveLength(2);
    expect(rows[1].classes()).toContain("recent");
    expect(rows[1].find(".identity-recent-badge").exists()).toBe(true);
  });

  it("clicking an identity value fills the active condition", async () => {
    const rule = makeRule({ conditions: [{ type: "executable", value: "" }] });
    const { body } = mountModal({
      modelValue: rule,
      identityStreams: [makeStream({ executable: "firefox" })],
    });

    const executableCell = body.findAll(".identity-value-btn")[1];
    await executableCell.trigger("click");

    expect(rule.conditions[0]).toEqual({ type: "executable", value: "firefox" });
  });
});
