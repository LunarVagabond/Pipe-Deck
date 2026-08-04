import { mount, flushPromises } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import Soundboard from "./Soundboard.vue";
import type { SoundboardBoard, SoundboardClip } from "../types/graph";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const openDialogMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openDialogMock }));

const pushNoticeMock = vi.hoisted(() => vi.fn());
vi.mock("../stores/notices", () => ({
  useApplyResult: () => ({
    handleApplyResult: (result: { success: boolean; message?: string }, successMessage: string) => {
      pushNoticeMock(result.success ? "success" : "error", result.success ? successMessage : result.message);
    },
  }),
}));

const promptMock = vi.hoisted(() => vi.fn());
vi.mock("../stores/prompt", () => ({
  usePrompt: () => ({ prompt: promptMock }),
}));

const confirmMock = vi.hoisted(() => vi.fn().mockResolvedValue(true));
vi.mock("../stores/confirm", () => ({
  useConfirm: () => ({ confirm: confirmMock }),
}));

function mockInvoke(handlers: Record<string, (args?: unknown) => unknown>) {
  invokeMock.mockImplementation((command: string, args?: unknown) => {
    const handler = handlers[command];
    if (!handler) throw new Error(`unexpected invoke: ${command}`);
    return Promise.resolve(handler(args));
  });
}

describe("Soundboard", () => {
  it("shows the empty state when there are no tabs", async () => {
    mockInvoke({ list_soundboard_boards: () => [] });
    const wrapper = mount(Soundboard);
    await flushPromises();

    expect(wrapper.find(".soundboard-empty-state").text()).toContain("No soundboard tabs yet.");
  });

  it("lists clips for the active tab", async () => {
    const boards: SoundboardBoard[] = [{ id: "b1", name: "SFX", folder: "/sounds/sfx", clip_targets: {} }];
    const clips: SoundboardClip[] = [
      { id: "air-horn.wav", file_name: "air-horn.wav", label: "air-horn", path: "/sounds/sfx/air-horn.wav" },
    ];
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: (args) => {
        expect((args as { boardId: string }).boardId).toBe("b1");
        return clips;
      },
    });
    const wrapper = mount(Soundboard);
    await flushPromises();

    expect(wrapper.find(".soundboard-grid").exists()).toBe(true);
    expect(wrapper.text()).toContain("air-horn");
    expect(wrapper.text()).toContain("SFX");
  });

  it("refresh button re-lists clips for the active tab", async () => {
    const boards: SoundboardBoard[] = [{ id: "b1", name: "SFX", folder: "/sounds/sfx", clip_targets: {} }];
    let clips: SoundboardClip[] = [];
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: () => clips,
    });
    const wrapper = mount(Soundboard);
    await flushPromises();
    expect(wrapper.text()).not.toContain("air-horn");

    clips = [{ id: "air-horn.wav", file_name: "air-horn.wav", label: "air-horn", path: "/sounds/sfx/air-horn.wav" }];
    const refreshButton = wrapper.findAll("button").find((btn) => btn.text() === "Refresh");
    await refreshButton?.trigger("click");
    await flushPromises();

    expect(wrapper.text()).toContain("air-horn");
  });

  it("adds a new tab via prompt + native folder picker", async () => {
    let boards: SoundboardBoard[] = [];
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: () => [],
      save_soundboard_board: (args) => {
        boards = [...boards, (args as { board: SoundboardBoard }).board];
      },
    });
    promptMock.mockResolvedValueOnce("Music");
    openDialogMock.mockResolvedValueOnce("/home/user/Music");

    const wrapper = mount(Soundboard);
    await flushPromises();

    const addButton = wrapper.findAll("button").find((btn) => btn.text() === "+ Add tab");
    await addButton?.trigger("click");
    await flushPromises();

    expect(openDialogMock).toHaveBeenCalledWith(expect.objectContaining({ directory: true }));
    expect(invokeMock).toHaveBeenCalledWith(
      "save_soundboard_board",
      expect.objectContaining({ board: expect.objectContaining({ name: "Music", folder: "/home/user/Music" }) }),
    );
    expect(pushNoticeMock).toHaveBeenCalledWith("success", 'Added "Music" tab');
  });

  it("shows the backend error message when listing fails", async () => {
    const boards: SoundboardBoard[] = [{ id: "b1", name: "SFX", folder: "/missing", clip_targets: {} }];
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: () => {
        throw new Error("soundboard folder not found: /missing");
      },
    });
    const wrapper = mount(Soundboard);
    await flushPromises();

    expect(wrapper.find(".status.error").text()).toContain("soundboard folder not found");
  });

  it("deletes the active tab after confirmation", async () => {
    let boards: SoundboardBoard[] = [{ id: "b1", name: "SFX", folder: "/sounds/sfx", clip_targets: {} }];
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: () => [],
      delete_soundboard_board: (args) => {
        boards = boards.filter((board) => board.id !== (args as { boardId: string }).boardId);
      },
    });
    const wrapper = mount(Soundboard);
    await flushPromises();

    const deleteButton = wrapper.findAll("button").find((btn) => btn.text() === "Delete tab");
    await deleteButton?.trigger("click");
    await flushPromises();

    expect(invokeMock).toHaveBeenCalledWith("delete_soundboard_board", { boardId: "b1" });
    expect(pushNoticeMock).toHaveBeenCalledWith("success", "Tab deleted");
  });

  it("clicking a tile with a target plays it", async () => {
    const boards: SoundboardBoard[] = [
      { id: "b1", name: "SFX", folder: "/sounds/sfx", clip_targets: { "air-horn.wav": "pipe-deck-stream-mic" } },
    ];
    const clips: SoundboardClip[] = [
      { id: "air-horn.wav", file_name: "air-horn.wav", label: "air-horn", path: "/sounds/sfx/air-horn.wav" },
    ];
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: () => clips,
      play_soundboard_clip: () => null,
    });
    const wrapper = mount(Soundboard);
    await flushPromises();

    const tile = wrapper.find(".soundboard-tile");
    expect(tile.classes()).not.toContain("no-target");
    await tile.trigger("click");
    await flushPromises();

    expect(invokeMock).toHaveBeenCalledWith("play_soundboard_clip", { boardId: "b1", clipId: "air-horn.wav" });
  });

  it("marks a tile with no target assigned and surfaces the backend error on click", async () => {
    const boards: SoundboardBoard[] = [{ id: "b1", name: "SFX", folder: "/sounds/sfx", clip_targets: {} }];
    const clips: SoundboardClip[] = [
      { id: "air-horn.wav", file_name: "air-horn.wav", label: "air-horn", path: "/sounds/sfx/air-horn.wav" },
    ];
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: () => clips,
      play_soundboard_clip: () => {
        throw new Error('"air-horn" has no target device set yet');
      },
    });
    const wrapper = mount(Soundboard);
    await flushPromises();

    const tile = wrapper.find(".soundboard-tile");
    expect(tile.classes()).toContain("no-target");
    await tile.trigger("click");
    await flushPromises();

    expect(pushNoticeMock).toHaveBeenCalledWith("error", '"air-horn" has no target device set yet');
  });
});
