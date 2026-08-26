import { mount, flushPromises } from "@vue/test-utils";
import { ref } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
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
    exclusive_playback: true,
    ...overrides,
  };
}

function makeClip(overrides: Partial<SoundboardClip> = {}): SoundboardClip {
  return {
    id: "clip.wav",
    file_name: "clip.wav",
    label: "clip",
    path: "/sounds/sfx/clip.wav",
    duration_seconds: null,
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
  beforeEach(() => {
    invokeMock.mockClear();
  });

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

  it("shows the legacy-restricted overlap policy and persists its toggle", async () => {
    let boards = [
      { ...makeBoard(), exclusive_playback: undefined },
    ] as unknown as SoundboardBoard[];
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: () => [],
      save_soundboard_board: (args) => {
        boards = [(args as { board: SoundboardBoard }).board];
      },
    });
    const wrapper = mount(Soundboard);
    await flushPromises();

    const toggle = wrapper.find("#soundboard-exclusive-playback");
    expect(toggle.exists()).toBe(true);
    expect((toggle.element as HTMLInputElement).checked).toBe(true);

    await toggle.setValue(false);
    await flushPromises();

    const savedBoard = invokeMock.mock.calls
      .filter((call) => call[0] === "save_soundboard_board")
      .at(-1)?.[1] as { board: Record<string, unknown> };
    expect(savedBoard.board.exclusive_playback).toBe(false);
    wrapper.unmount();
  });

  it("allows a second clip without stopping the first in overlap mode", async () => {
    const boards = [
      {
        ...makeBoard({ target_system_name: "pipe-deck-stream-mic" }),
        exclusive_playback: false,
      },
    ] as SoundboardBoard[];
    const clips: SoundboardClip[] = [
      {
        id: "first.wav",
        file_name: "first.wav",
        label: "first",
        path: "/sounds/sfx/first.wav",
        duration_seconds: null,
      },
      {
        id: "second.wav",
        file_name: "second.wav",
        label: "second",
        path: "/sounds/sfx/second.wav",
        duration_seconds: null,
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

    const tiles = wrapper.findAll(".soundboard-tile");
    expect(tiles[1].attributes("disabled")).toBeUndefined();
    await tiles[0].trigger("click");
    await flushPromises();
    await tiles[1].trigger("click");
    await flushPromises();

    expect(
      invokeMock.mock.calls.filter(
        (call) => call[0] === "play_soundboard_clip",
      ),
    ).toHaveLength(2);
    expect(
      invokeMock.mock.calls.some((call) => call[0] === "stop_soundboard_clip"),
    ).toBe(false);
    wrapper.unmount();
  });

  it("applies and persists each board's playback policy independently", async () => {
    let wrapper: ReturnType<typeof mount> | undefined;
    let boards: SoundboardBoard[] = [
      makeBoard({
        id: "board-overlap",
        name: "Overlap",
        folder: "/sounds/overlap",
        target_system_name: "pipe-deck-stream-mic",
        exclusive_playback: false,
      }),
      makeBoard({
        id: "board-exclusive",
        name: "Exclusive",
        folder: "/sounds/exclusive",
        target_system_name: "pipe-deck-stream-mic",
        exclusive_playback: true,
      }),
    ];
    const clipsByBoard: Record<string, SoundboardClip[]> = {
      "board-overlap": [
        makeClip({ id: "overlap-first.wav", file_name: "overlap-first.wav" }),
        makeClip({
          id: "overlap-second.wav",
          file_name: "overlap-second.wav",
        }),
      ],
      "board-exclusive": [
        makeClip({
          id: "exclusive-first.wav",
          file_name: "exclusive-first.wav",
        }),
        makeClip({
          id: "exclusive-second.wav",
          file_name: "exclusive-second.wav",
        }),
      ],
    };
    const commandSequence: string[] = [];
    const savedBoardPayloads: SoundboardBoard[] = [];
    let deferStop = false;
    let releaseStop: (() => void) | undefined;
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: (args) =>
        clipsByBoard[(args as { boardId: string }).boardId],
      save_soundboard_board: (args) => {
        const board = (args as { board: SoundboardBoard }).board;
        savedBoardPayloads.push({ ...board });
        boards = boards.map((existing) =>
          existing.id === board.id ? { ...existing, ...board } : existing,
        );
      },
      play_soundboard_clip: (args) => {
        const { boardId, clipId } = args as {
          boardId: string;
          clipId: string;
        };
        commandSequence.push(`play:${boardId}:${clipId}`);
        return null;
      },
      stop_soundboard_clip: () => {
        commandSequence.push("stop");
        if (deferStop) {
          return new Promise<void>((resolve) => {
            releaseStop = resolve;
          });
        }
        return null;
      },
    });

    try {
      wrapper = mount(Soundboard);
      await flushPromises();
      const boardTabs = wrapper
        .find(".soundboard-view > .segmented-control")
        .findAll(".segmented-control-option");
      expect(boardTabs).toHaveLength(3);
      expect(boardTabs[0].text()).toBe("Overlap");
      expect(boardTabs[1].text()).toBe("Exclusive");
      expect(wrapper.find(".soundboard-board-folder").text()).toBe(
        "/sounds/overlap",
      );

      let tiles = wrapper.findAll(".soundboard-tile");
      expect(tiles).toHaveLength(2);
      await tiles[0].trigger("click");
      await flushPromises();
      await tiles[1].trigger("click");
      await flushPromises();
      expect(commandSequence).toEqual([
        "play:board-overlap:overlap-first.wav",
        "play:board-overlap:overlap-second.wav",
      ]);

      await tiles[0].trigger("click");
      await flushPromises();
      commandSequence.length = 0;
      deferStop = true;

      await boardTabs[1].trigger("click");
      await flushPromises();
      expect(wrapper.find(".soundboard-board-folder").text()).toBe(
        "/sounds/exclusive",
      );
      tiles = wrapper.findAll(".soundboard-tile");
      expect(tiles).toHaveLength(2);
      await tiles[0].trigger("click");
      await flushPromises();
      expect(commandSequence).toEqual([
        "play:board-exclusive:exclusive-first.wav",
      ]);

      const replacementClick = tiles[1].trigger("click");
      await flushPromises();
      expect(releaseStop).toEqual(expect.any(Function));
      expect(commandSequence).toEqual([
        "play:board-exclusive:exclusive-first.wav",
        "stop",
      ]);
      releaseStop?.();
      await replacementClick;
      await flushPromises();
      expect(commandSequence).toEqual([
        "play:board-exclusive:exclusive-first.wav",
        "stop",
        "play:board-exclusive:exclusive-second.wav",
      ]);

      const toggle = wrapper.find("#soundboard-exclusive-playback");
      expect((toggle.element as HTMLInputElement).checked).toBe(true);
      await toggle.setValue(false);
      await flushPromises();
      const toggleAfterFirstSave = wrapper.find(
        "#soundboard-exclusive-playback",
      );
      expect((toggleAfterFirstSave.element as HTMLInputElement).checked).toBe(
        false,
      );
      await toggleAfterFirstSave.setValue(true);
      await flushPromises();
      expect(savedBoardPayloads).toHaveLength(2);
      expect(
        savedBoardPayloads.map(({ id, exclusive_playback }) => ({
          id,
          exclusive_playback,
        })),
      ).toEqual([
        { id: "board-exclusive", exclusive_playback: false },
        { id: "board-exclusive", exclusive_playback: true },
      ]);
      expect(
        boards.find((board) => board.id === "board-overlap")
          ?.exclusive_playback,
      ).toBe(false);
      expect(
        boards.find((board) => board.id === "board-exclusive")
          ?.exclusive_playback,
      ).toBe(true);

      const refreshedBoardTabs = wrapper
        .find(".soundboard-view > .segmented-control")
        .findAll(".segmented-control-option");
      expect(wrapper.find(".soundboard-board-folder").text()).toBe(
        "/sounds/exclusive",
      );
      expect(
        (
          wrapper.find("#soundboard-exclusive-playback")
            .element as HTMLInputElement
        ).checked,
      ).toBe(true);
      await refreshedBoardTabs[0].trigger("click");
      await flushPromises();
      expect(wrapper.find(".soundboard-board-folder").text()).toBe(
        "/sounds/overlap",
      );
      expect(
        (
          wrapper.find("#soundboard-exclusive-playback")
            .element as HTMLInputElement
        ).checked,
      ).toBe(false);
      const finalBoardTabs = wrapper
        .find(".soundboard-view > .segmented-control")
        .findAll(".segmented-control-option");
      await finalBoardTabs[1].trigger("click");
      await flushPromises();
      expect(wrapper.find(".soundboard-board-folder").text()).toBe(
        "/sounds/exclusive",
      );
      expect(
        (
          wrapper.find("#soundboard-exclusive-playback")
            .element as HTMLInputElement
        ).checked,
      ).toBe(true);
    } finally {
      wrapper?.unmount();
    }
  });

  it("does not inherit same-name playback across overlapping soundboard tabs", async () => {
    const boards: SoundboardBoard[] = [
      makeBoard({
        id: "board-a",
        name: "Board A",
        folder: "/sounds/a",
        exclusive_playback: false,
      }),
      makeBoard({
        id: "board-b",
        name: "Board B",
        folder: "/sounds/b",
        exclusive_playback: false,
      }),
    ];
    const clipA = makeClip({
      id: "alert.wav",
      file_name: "alert.wav",
      label: "alert.wav",
      duration_seconds: 4,
    });
    const clipB = makeClip({
      id: "alert.wav",
      file_name: "alert.wav",
      label: "alert.wav",
      duration_seconds: 4,
    });
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: (args) =>
        (args as { boardId: string }).boardId === "board-a" ? [clipA] : [clipB],
      play_soundboard_clip: () => null,
      stop_soundboard_clip: () => null,
    });

    const wrapper = mount(Soundboard);
    await flushPromises();

    const boardTabs = wrapper
      .find(".soundboard-view > .segmented-control")
      .findAll(".segmented-control-option");
    expect(boardTabs).toHaveLength(3);

    let tile = wrapper.find(".soundboard-tile");
    await tile.trigger("click");
    await flushPromises();
    expect(tile.classes()).toContain("playing");

    await boardTabs![1].trigger("click");
    await flushPromises();
    tile = wrapper.find(".soundboard-tile");
    expect(tile.classes()).not.toContain("playing");

    await tile.trigger("click");
    await flushPromises();
    expect(tile.classes()).toContain("playing");
    expect(
      invokeMock.mock.calls.filter(([name]) => name === "play_soundboard_clip"),
    ).toHaveLength(2);
    expect(
      invokeMock.mock.calls.filter(([name]) => name === "stop_soundboard_clip"),
    ).toHaveLength(0);

    wrapper.unmount();
  });

  it("retains playing progress independently for same-name overlap clips on both tabs", async () => {
    let wrapper: ReturnType<typeof mount> | undefined;
    vi.useFakeTimers();
    try {
      vi.setSystemTime(new Date(0));
      const boards: SoundboardBoard[] = [
        makeBoard({
          id: "board-a",
          name: "Board A",
          folder: "/sounds/a",
          exclusive_playback: false,
        }),
        makeBoard({
          id: "board-b",
          name: "Board B",
          folder: "/sounds/b",
          exclusive_playback: false,
        }),
      ];
      const clipsByBoard: Record<string, SoundboardClip[]> = {
        "board-a": [
          makeClip({
            id: "alert.wav",
            file_name: "alert.wav",
            duration_seconds: 4,
          }),
        ],
        "board-b": [
          makeClip({
            id: "alert.wav",
            file_name: "alert.wav",
            duration_seconds: 7,
          }),
        ],
      };
      mockInvoke({
        list_soundboard_boards: () => boards,
        list_soundboard_sounds: (args) =>
          clipsByBoard[(args as { boardId: string }).boardId],
        play_soundboard_clip: () => null,
        stop_soundboard_clip: () => null,
      });

      const mountedWrapper: ReturnType<typeof mount> = (wrapper =
        mount(Soundboard));
      await flushPromises();
      const boardTabs = mountedWrapper
        .find(".soundboard-view > .segmented-control")
        .findAll(".segmented-control-option");
      expect(boardTabs).toHaveLength(3);
      const switchBoardAndAdvance = async (index: number) => {
        await boardTabs[index].trigger("click");
        await flushPromises();
        await vi.advanceTimersByTimeAsync(200);
        await flushPromises();
      };

      await mountedWrapper.find(".soundboard-tile").trigger("click");
      await flushPromises();
      await vi.advanceTimersByTimeAsync(800);
      await flushPromises();

      await switchBoardAndAdvance(1);
      await mountedWrapper.find(".soundboard-tile").trigger("click");
      await flushPromises();
      await vi.advanceTimersByTimeAsync(1200);
      await flushPromises();

      let tile = mountedWrapper.find(".soundboard-tile");
      expect(tile.classes()).toContain("playing");
      expect(tile.find(".soundboard-tile-progress").exists()).toBe(true);
      let progressTimes = tile.findAll(
        ".soundboard-tile-progress-times > span",
      );
      expect(progressTimes).toHaveLength(2);
      expect(progressTimes[0].text()).toBe("0:01");
      expect(progressTimes[1].text()).toBe("0:07");

      await switchBoardAndAdvance(0);
      tile = mountedWrapper.find(".soundboard-tile");
      expect(tile.classes()).toContain("playing");
      expect(tile.find(".soundboard-tile-progress").exists()).toBe(true);
      progressTimes = tile.findAll(".soundboard-tile-progress-times > span");
      expect(progressTimes).toHaveLength(2);
      expect(progressTimes[0].text()).toBe("0:02");
      expect(progressTimes[1].text()).toBe("0:04");

      await switchBoardAndAdvance(1);
      tile = mountedWrapper.find(".soundboard-tile");
      expect(tile.classes()).toContain("playing");
      expect(tile.find(".soundboard-tile-progress").exists()).toBe(true);
      progressTimes = tile.findAll(".soundboard-tile-progress-times > span");
      expect(progressTimes).toHaveLength(2);
      expect(progressTimes[0].text()).toBe("0:02");
      expect(progressTimes[1].text()).toBe("0:07");

      await switchBoardAndAdvance(0);
      tile = mountedWrapper.find(".soundboard-tile");
      expect(tile.classes()).toContain("playing");
      progressTimes = tile.findAll(".soundboard-tile-progress-times > span");
      expect(progressTimes).toHaveLength(2);
      expect(progressTimes[0].text()).toBe("0:03");
      expect(progressTimes[1].text()).toBe("0:04");
    } finally {
      wrapper?.unmount();
      vi.useRealTimers();
    }
  });

  it("surfaces a restricted stop failure and suppresses the next play", async () => {
    const board = makeBoard({
      id: "board-a",
      name: "Board A",
      folder: "/sounds/a",
      exclusive_playback: true,
    });
    const clips = [
      makeClip({ id: "first.wav", file_name: "first.wav" }),
      makeClip({ id: "second.wav", file_name: "second.wav" }),
    ];
    mockInvoke({
      list_soundboard_boards: () => [board],
      list_soundboard_sounds: () => clips,
      play_soundboard_clip: () => null,
      stop_soundboard_clip: () => {
        throw new Error("stop failed");
      },
    });

    const wrapper = mount(Soundboard);
    await flushPromises();
    const tiles = wrapper.findAll(".soundboard-tile");
    await tiles[0].trigger("click");
    await flushPromises();
    await tiles[1].trigger("click");
    await flushPromises();

    expect(
      invokeMock.mock.calls.filter(([name]) => name === "stop_soundboard_clip"),
    ).toHaveLength(1);
    expect(
      invokeMock.mock.calls.filter(([name]) => name === "play_soundboard_clip"),
    ).toHaveLength(1);
    expect(pushNoticeMock).toHaveBeenCalledWith("error", "stop failed");

    wrapper.unmount();
  });

  it("stops the current clip before starting the next in restricted mode", async () => {
    const boards = [
      {
        ...makeBoard({ target_system_name: "pipe-deck-stream-mic" }),
        exclusive_playback: true,
      },
    ] as SoundboardBoard[];
    const clips: SoundboardClip[] = [
      {
        id: "first.wav",
        file_name: "first.wav",
        label: "first",
        path: "/sounds/sfx/first.wav",
        duration_seconds: null,
      },
      {
        id: "second.wav",
        file_name: "second.wav",
        label: "second",
        path: "/sounds/sfx/second.wav",
        duration_seconds: null,
      },
    ];
    const calls: string[] = [];
    mockInvoke({
      list_soundboard_boards: () => boards,
      list_soundboard_sounds: () => clips,
      play_soundboard_clip: () => {
        calls.push("play");
        return null;
      },
      stop_soundboard_clip: () => {
        calls.push("stop");
        return null;
      },
    });
    const wrapper = mount(Soundboard);
    await flushPromises();

    const tiles = wrapper.findAll(".soundboard-tile");
    expect(tiles[1].attributes("disabled")).toBeUndefined();
    await tiles[0].trigger("click");
    await flushPromises();
    await tiles[1].trigger("click");
    await flushPromises();

    expect(calls).toEqual(["play", "stop", "play"]);
    wrapper.unmount();
  });
});
