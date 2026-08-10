import { mount, flushPromises } from "@vue/test-utils";
import { ref } from "vue";
import { describe, expect, it, vi } from "vitest";
import Soundboard from "./Soundboard.vue";
import type {
  Device,
  RuntimeGraph,
  SoundboardBoard,
  SoundboardClip,
} from "../types/graph";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const openDialogMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openDialogMock }));

const pushNoticeMock = vi.hoisted(() => vi.fn());
vi.mock("../stores/notices", () => ({
  useApplyResult: () => ({
    handleApplyResult: (
      result: { success: boolean; message?: string },
      successMessage: string,
    ) => {
      pushNoticeMock(
        result.success ? "success" : "error",
        result.success ? successMessage : result.message,
      );
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

// jsdom in this project's vitest config doesn't back window.localStorage by
// default (see RoutingGraph.spec.ts's note) — Soundboard.vue reads/writes it
// directly for the clip layout/card size preference, so a minimal in-memory
// stub is needed just to get past mount.
const localStorageStore = new Map<string, string>();
vi.stubGlobal("localStorage", {
  getItem: (key: string) => localStorageStore.get(key) ?? null,
  setItem: (key: string, value: string) => localStorageStore.set(key, value),
  removeItem: (key: string) => localStorageStore.delete(key),
  clear: () => localStorageStore.clear(),
});

function makeDevice(overrides: Partial<Device> = {}): Device {
  return {
    id: "device-1",
    system_name: "pipe-deck-stream-mic",
    label: "Stream Mic",
    kind: "virtual",
    direction: "input",
    ...overrides,
  } as Device;
}

const graph = ref<RuntimeGraph>({
  devices: [
    makeDevice({
      id: "mic",
      system_name: "pipe-deck-stream-mic",
      label: "Stream Mic",
      direction: "input",
    }),
    makeDevice({
      id: "hdmi",
      system_name: "alsa_output.pci-hdmi",
      label: "HDMI Speakers",
      direction: "output",
    }),
  ],
  streams: [],
  links: [],
});
vi.mock("../stores/runtimeGraph", () => ({
  useRuntimeGraph: () => ({ graph }),
}));

function makeBoard(overrides: Partial<SoundboardBoard> = {}): SoundboardBoard {
  return {
    id: "b1",
    name: "SFX",
    folder: "/sounds/sfx",
    target_system_name: null,
    target_volume_percent: 100,
    monitor_system_name: null,
    monitor_volume_percent: 100,
    ...overrides,
  };
}

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

    expect(wrapper.find(".soundboard-empty-state").text()).toContain(
      "No soundboard tabs yet.",
    );
  });

  it("lists clips for the active tab", async () => {
    const boards: SoundboardBoard[] = [makeBoard()];
    const clips: SoundboardClip[] = [
      {
        id: "air-horn.wav",
        file_name: "air-horn.wav",
        label: "air-horn",
        path: "/sounds/sfx/air-horn.wav",
        duration_seconds: 3,
      },
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
    const boards: SoundboardBoard[] = [makeBoard()];
    let clips: SoundboardClip[] = [];
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: () => clips,
    });
    const wrapper = mount(Soundboard);
    await flushPromises();
    expect(wrapper.text()).not.toContain("air-horn");

    clips = [
      {
        id: "air-horn.wav",
        file_name: "air-horn.wav",
        label: "air-horn",
        path: "/sounds/sfx/air-horn.wav",
        duration_seconds: 3,
      },
    ];
    const refreshButton = wrapper
      .findAll("button")
      .find((btn) => btn.text() === "Refresh");
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

    const addButton = wrapper
      .findAll("button")
      .find((btn) => btn.text() === "+ Add tab");
    await addButton?.trigger("click");
    await flushPromises();

    expect(openDialogMock).toHaveBeenCalledWith(
      expect.objectContaining({ directory: true }),
    );
    expect(invokeMock).toHaveBeenCalledWith(
      "save_soundboard_board",
      expect.objectContaining({
        board: expect.objectContaining({
          name: "Music",
          folder: "/home/user/Music",
        }),
      }),
    );
    expect(pushNoticeMock).toHaveBeenCalledWith("success", 'Added "Music" tab');
  });

  it("shows the backend error message when listing fails", async () => {
    const boards: SoundboardBoard[] = [makeBoard({ folder: "/missing" })];
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: () => {
        throw new Error("soundboard folder not found: /missing");
      },
    });
    const wrapper = mount(Soundboard);
    await flushPromises();

    expect(wrapper.find(".status.error").text()).toContain(
      "soundboard folder not found",
    );
  });

  it("deletes the active tab after confirmation", async () => {
    let boards: SoundboardBoard[] = [makeBoard()];
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: () => [],
      delete_soundboard_board: (args) => {
        boards = boards.filter(
          (board) => board.id !== (args as { boardId: string }).boardId,
        );
      },
    });
    const wrapper = mount(Soundboard);
    await flushPromises();

    const deleteButton = wrapper
      .findAll("button")
      .find((btn) => btn.text() === "Delete tab");
    await deleteButton?.trigger("click");
    await flushPromises();

    expect(invokeMock).toHaveBeenCalledWith("delete_soundboard_board", {
      boardId: "b1",
    });
    expect(pushNoticeMock).toHaveBeenCalledWith("success", "Tab deleted");
  });

  it("clicking a tile plays it when the tab has a destination configured", async () => {
    const boards: SoundboardBoard[] = [
      makeBoard({ target_system_name: "pipe-deck-stream-mic" }),
    ];
    const clips: SoundboardClip[] = [
      {
        id: "air-horn.wav",
        file_name: "air-horn.wav",
        label: "air-horn",
        path: "/sounds/sfx/air-horn.wav",
        duration_seconds: 3,
      },
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

    expect(invokeMock).toHaveBeenCalledWith("play_soundboard_clip", {
      boardId: "b1",
      clipId: "air-horn.wav",
    });
  });

  it("shows a progress bar with elapsed/remaining time while a clip plays, and stops it on a second click", async () => {
    vi.useFakeTimers();
    try {
      const boards: SoundboardBoard[] = [
        makeBoard({ target_system_name: "pipe-deck-stream-mic" }),
      ];
      const clips: SoundboardClip[] = [
        {
          id: "air-horn.wav",
          file_name: "air-horn.wav",
          label: "air-horn",
          path: "/sounds/sfx/air-horn.wav",
          duration_seconds: 4,
        },
      ];
      mockInvoke({
        list_soundboard_boards: () => boards,
        list_soundboard_sounds: () => clips,
        play_soundboard_clip: () => null,
        stop_soundboard_clip: () => null,
      });
      const wrapper = mount(Soundboard);
      await flushPromises();

      const tile = wrapper.find(".soundboard-tile");
      await tile.trigger("click");
      await flushPromises();

      expect(tile.classes()).toContain("playing");
      expect(wrapper.find(".soundboard-tile-progress").exists()).toBe(true);
      expect(wrapper.find(".soundboard-tile-progress-times").text()).toContain(
        "0:00",
      );
      expect(wrapper.find(".soundboard-tile-progress-times").text()).toContain(
        "0:04",
      );

      await vi.advanceTimersByTimeAsync(2000);
      await flushPromises();
      expect(wrapper.find(".soundboard-tile-progress-times").text()).toContain(
        "0:02",
      );
      // The right-hand number is the clip's static length, not a countdown.
      expect(wrapper.find(".soundboard-tile-progress-times").text()).toContain(
        "0:04",
      );

      await tile.trigger("click");
      await flushPromises();

      expect(invokeMock).toHaveBeenCalledWith("stop_soundboard_clip");
      expect(wrapper.find(".soundboard-tile-progress").exists()).toBe(false);
      expect(tile.classes()).not.toContain("playing");
    } finally {
      vi.useRealTimers();
    }
  });

  it("clears playback state on its own once elapsed time reaches the clip's duration", async () => {
    vi.useFakeTimers();
    try {
      const boards: SoundboardBoard[] = [
        makeBoard({ target_system_name: "pipe-deck-stream-mic" }),
      ];
      const clips: SoundboardClip[] = [
        {
          id: "air-horn.wav",
          file_name: "air-horn.wav",
          label: "air-horn",
          path: "/sounds/sfx/air-horn.wav",
          duration_seconds: 1,
        },
      ];
      mockInvoke({
        list_soundboard_boards: () => boards,
        list_soundboard_sounds: () => clips,
        play_soundboard_clip: () => null,
      });
      const wrapper = mount(Soundboard);
      await flushPromises();

      const tile = wrapper.find(".soundboard-tile");
      await tile.trigger("click");
      await flushPromises();
      expect(tile.classes()).toContain("playing");

      await vi.advanceTimersByTimeAsync(1200);
      await flushPromises();

      expect(tile.classes()).not.toContain("playing");
      expect(wrapper.find(".soundboard-tile-progress").exists()).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("switches between cards and list layout, and adjusts card size", async () => {
    const boards: SoundboardBoard[] = [makeBoard()];
    const clips: SoundboardClip[] = [
      {
        id: "air-horn.wav",
        file_name: "air-horn.wav",
        label: "air-horn",
        path: "/sounds/sfx/air-horn.wav",
        duration_seconds: 3,
      },
    ];
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: () => clips,
    });
    const wrapper = mount(Soundboard);
    await flushPromises();

    // Cards layout, medium size by default.
    expect(wrapper.find(".soundboard-grid").exists()).toBe(true);
    expect(wrapper.find(".soundboard-grid--medium").exists()).toBe(true);

    const sizeButtons = wrapper.findAll(
      ".soundboard-layout-toolbar .segmented-control-option",
    );
    await sizeButtons[2].trigger("click"); // large
    await flushPromises();
    expect(wrapper.find(".soundboard-grid--large").exists()).toBe(true);

    const listButton = wrapper
      .findAll(".soundboard-layout-toolbar .segmented-control-option")
      .at(-1);
    await listButton?.trigger("click");
    await flushPromises();

    expect(wrapper.find(".soundboard-list").exists()).toBe(true);
    expect(wrapper.find(".soundboard-grid").exists()).toBe(false);
    expect(wrapper.find(".soundboard-tile").classes()).toContain(
      "soundboard-tile--list",
    );
    // The size selector stays mounted (not removed) so the toolbar doesn't
    // jump around when toggling layout, just disabled while in list mode.
    const [smallButton] = wrapper.findAll(
      ".soundboard-layout-toolbar .segmented-control-option",
    );
    expect(smallButton.attributes("disabled")).toBeDefined();
  });

  it("marks tiles dimmed when the tab has no destination and surfaces the backend error on click", async () => {
    const boards: SoundboardBoard[] = [makeBoard()];
    const clips: SoundboardClip[] = [
      {
        id: "air-horn.wav",
        file_name: "air-horn.wav",
        label: "air-horn",
        path: "/sounds/sfx/air-horn.wav",
        duration_seconds: 3,
      },
    ];
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: () => clips,
      play_soundboard_clip: () => {
        throw new Error('"SFX" tab has no target or monitor device set yet');
      },
    });
    const wrapper = mount(Soundboard);
    await flushPromises();

    const tile = wrapper.find(".soundboard-tile");
    expect(tile.classes()).toContain("no-target");
    await tile.trigger("click");
    await flushPromises();

    expect(pushNoticeMock).toHaveBeenCalledWith(
      "error",
      '"SFX" tab has no target or monitor device set yet',
    );
  });

  it("saves the tab's target device on selection", async () => {
    let boards: SoundboardBoard[] = [makeBoard()];
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: () => [],
      save_soundboard_board: (args) => {
        boards = [(args as { board: SoundboardBoard }).board];
      },
    });
    const wrapper = mount(Soundboard);
    await flushPromises();

    await wrapper
      .find("#soundboard-target-device")
      .setValue("pipe-deck-stream-mic");
    await flushPromises();

    const savedBoard = invokeMock.mock.calls
      .filter((call) => call[0] === "save_soundboard_board")
      .at(-1)?.[1] as {
      board: SoundboardBoard;
    };
    expect(savedBoard.board.target_system_name).toBe("pipe-deck-stream-mic");
  });

  it("saves the tab's monitor device and volume", async () => {
    let boards: SoundboardBoard[] = [makeBoard()];
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: () => [],
      save_soundboard_board: (args) => {
        boards = [(args as { board: SoundboardBoard }).board];
      },
    });
    const wrapper = mount(Soundboard);
    await flushPromises();

    await wrapper
      .find("#soundboard-monitor-device")
      .setValue("alsa_output.pci-hdmi");
    await flushPromises();

    const savedBoard = invokeMock.mock.calls
      .filter((call) => call[0] === "save_soundboard_board")
      .at(-1)?.[1] as {
      board: SoundboardBoard;
    };
    expect(savedBoard.board.monitor_system_name).toBe("alsa_output.pci-hdmi");
  });
});
