import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { mount } from "@vue/test-utils";
import MixerStrip from "./MixerStrip.vue";
import { makeDevice, makeStream } from "../test/graphFixtures";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue({ success: true });
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("MixerStrip keyboard/focus accessibility", () => {
  it("renders the volume fader as a native tabbable range input with a descriptive label", () => {
    const device = makeDevice({ id: "dev-1", label: "Speakers", volume_percent: 42 });
    const wrapper = mount(MixerStrip, { props: { devices: [device] } });

    const fader = wrapper.get(".volume-horizontal");
    expect(fader.element.tagName).toBe("INPUT");
    expect(fader.attributes("type")).toBe("range");
    expect(fader.attributes("aria-label")).toBe("Speakers volume");
    // Native range inputs are keyboard-operable (arrow keys) without extra wiring,
    // as long as nothing has knocked them out of tab order.
    expect(fader.attributes("tabindex")).not.toBe("-1");
  });

  it("keeps every interactive control in natural tab order (none excluded via tabindex=-1)", () => {
    const device = makeDevice({ id: "dev-1", label: "Speakers", volume_percent: 42 });
    const wrapper = mount(MixerStrip, { props: { devices: [device] } });

    const interactive = wrapper.findAll("button, input, select, [role=slider]");
    expect(interactive.length).toBeGreaterThan(0);
    for (const el of interactive) {
      expect(el.attributes("tabindex")).not.toBe("-1");
    }
  });

  it("adjusting the fader (e.g. via arrow keys, which fire a native input event) applies the new volume", async () => {
    const device = makeDevice({ id: "dev-1", label: "Speakers", volume_percent: 42 });
    const wrapper = mount(MixerStrip, { props: { devices: [device] } });

    const fader = wrapper.get(".volume-horizontal");
    await fader.setValue("55");

    await vi.advanceTimersByTimeAsync(150);

    expect(invokeMock).toHaveBeenCalledWith("set_device_volume", { deviceId: "dev-1", percent: 55 });
  });

  it("mute button is a real button with a state-describing aria-label and is keyboard-activatable", async () => {
    const device = makeDevice({ id: "dev-1", label: "Speakers", volume_percent: 42, muted: false });
    const wrapper = mount(MixerStrip, { props: { devices: [device] } });

    const mute = wrapper.get(".mute");
    expect(mute.element.tagName).toBe("BUTTON");
    expect(mute.attributes("aria-label")).toBe("Unmuted");

    await mute.trigger("click");
    expect(invokeMock).toHaveBeenCalledWith("set_device_mute", { deviceId: "dev-1", muted: true });
  });

  it("stream channels also expose a labelled, focusable fader", () => {
    const stream = makeStream({ id: "stream-1", app_name: "Discord", volume_percent: 70 });
    const wrapper = mount(MixerStrip, { props: { devices: [], streams: [stream] } });

    const fader = wrapper.get(".volume-horizontal");
    expect(fader.attributes("aria-label")).toBe("Discord volume");
  });
});
