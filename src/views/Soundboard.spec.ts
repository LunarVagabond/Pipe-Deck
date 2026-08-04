import { mount, flushPromises } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import Soundboard from "./Soundboard.vue";
import type { SoundboardClip } from "../types/graph";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const pushNoticeMock = vi.hoisted(() => vi.fn());
vi.mock("../stores/notices", () => ({
  useApplyResult: () => ({
    handleApplyResult: (result: { success: boolean; message?: string }, successMessage: string) => {
      pushNoticeMock(result.success ? "success" : "error", result.success ? successMessage : result.message);
    },
  }),
}));

function mockInvoke(handlers: Record<string, (args?: unknown) => unknown>) {
  invokeMock.mockImplementation((command: string, args?: unknown) => {
    const handler = handlers[command];
    if (!handler) throw new Error(`unexpected invoke: ${command}`);
    return Promise.resolve(handler(args));
  });
}

describe("Soundboard", () => {
  it("shows the empty state when no folder is configured", async () => {
    mockInvoke({ get_soundboard_folder: () => null });
    const wrapper = mount(Soundboard);
    await flushPromises();

    expect(wrapper.find(".soundboard-empty-state").text()).toContain("No sound clips configured yet.");
  });

  it("lists clips returned for a configured folder", async () => {
    const clips: SoundboardClip[] = [
      { id: "air-horn.wav", file_name: "air-horn.wav", label: "air-horn", path: "/sounds/air-horn.wav" },
    ];
    mockInvoke({
      get_soundboard_folder: () => "/sounds",
      list_soundboard_sounds: () => clips,
    });
    const wrapper = mount(Soundboard);
    await flushPromises();

    expect(wrapper.find(".soundboard-grid").exists()).toBe(true);
    expect(wrapper.text()).toContain("air-horn");
  });

  it("shows the backend error message when listing fails", async () => {
    mockInvoke({
      get_soundboard_folder: () => "/missing",
      list_soundboard_sounds: () => {
        throw new Error("soundboard folder not found: /missing");
      },
    });
    const wrapper = mount(Soundboard);
    await flushPromises();

    expect(wrapper.find(".status.error").text()).toContain("soundboard folder not found");
  });

  it("saves a new folder and reloads clips", async () => {
    let folder: string | null = null;
    mockInvoke({
      get_soundboard_folder: () => folder,
      set_soundboard_folder: (args) => {
        folder = (args as { folder: string }).folder;
      },
      list_soundboard_sounds: () => [],
    });
    const wrapper = mount(Soundboard);
    await flushPromises();

    await wrapper.find("#soundboard-folder-input").setValue("/home/user/Sounds");
    await wrapper.find(".soundboard-folder-form").trigger("submit");
    await flushPromises();

    expect(invokeMock).toHaveBeenCalledWith("set_soundboard_folder", { folder: "/home/user/Sounds" });
    expect(pushNoticeMock).toHaveBeenCalledWith("success", "Soundboard folder saved");
  });
});
