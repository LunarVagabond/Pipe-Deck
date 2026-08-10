<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import SegmentedControl from "../components/SegmentedControl.vue";
import { useApplyResult } from "../stores/notices";
import { usePrompt } from "../stores/prompt";
import { useConfirm } from "../stores/confirm";
import { useRuntimeGraph } from "../stores/runtimeGraph";
import type { SoundboardBoard, SoundboardClip } from "../types/graph";

const ADD_TAB_OPTION = "__add_tab__";

// Clip layout is a cosmetic display preference, not backend state — same
// lightweight localStorage-backed approach as the routing graph's group
// expansion (`composables/groupExpansion.ts`), no store/composable needed
// for a single view.
type ClipLayout = "cards" | "list";
type CardSize = "small" | "medium" | "large";
const LAYOUT_KEY = "pipe-deck-soundboard-layout";
const CARD_SIZE_KEY = "pipe-deck-soundboard-card-size";

function loadClipLayout(): ClipLayout {
  return localStorage.getItem(LAYOUT_KEY) === "list" ? "list" : "cards";
}

function loadCardSize(): CardSize {
  const stored = localStorage.getItem(CARD_SIZE_KEY);
  return stored === "small" || stored === "large" ? stored : "medium";
}

const clipLayout = ref<ClipLayout>(loadClipLayout());
const cardSize = ref<CardSize>(loadCardSize());

const layoutOptions = [
  { value: "cards", label: "▦" },
  { value: "list", label: "☰" },
];
const cardSizeOptions = [
  { value: "small", label: "S" },
  { value: "medium", label: "M" },
  { value: "large", label: "L" },
];

function selectClipLayout(value: string) {
  clipLayout.value = value === "list" ? "list" : "cards";
  localStorage.setItem(LAYOUT_KEY, clipLayout.value);
}

function selectCardSize(value: string) {
  cardSize.value = value === "small" || value === "large" ? value : "medium";
  localStorage.setItem(CARD_SIZE_KEY, cardSize.value);
}

const { handleApplyResult } = useApplyResult();
const { prompt } = usePrompt();
const { confirm } = useConfirm();
const { graph } = useRuntimeGraph();

const boards = ref<SoundboardBoard[]>([]);
const activeBoardId = ref<string | null>(null);
const clips = ref<SoundboardClip[]>([]);
const loadingBoards = ref(true);
const loadingClips = ref(false);
const error = ref<string | null>(null);
const playingClipId = ref<string | null>(null);
// Elapsed/remaining time is interpolated client-side from the clip's probed
// duration (#399) rather than pushed from the backend — `play_soundboard_clip`
// only ever tells us playback *started* (PD-036), never when it finishes, so
// there's nothing to subscribe to. `playingDurationSeconds` is null when the
// clip's duration couldn't be probed (see `SoundboardClip.duration_seconds`);
// the progress bar degrades to an elapsed-only counter with no fixed end.
const playingDurationSeconds = ref<number | null>(null);
const playingElapsedSeconds = ref(0);
let playingStartedAt = 0;
let progressTimer: ReturnType<typeof setInterval> | null = null;
const PROGRESS_TICK_MS = 200;

function clearProgressTimer() {
  if (progressTimer !== null) {
    clearInterval(progressTimer);
    progressTimer = null;
  }
}

function resetPlaybackState() {
  clearProgressTimer();
  playingClipId.value = null;
  playingDurationSeconds.value = null;
  playingElapsedSeconds.value = 0;
}

onUnmounted(clearProgressTimer);

const playingProgressPercent = computed(() => {
  const duration = playingDurationSeconds.value;
  if (!duration) return 0;
  return Math.min(100, (playingElapsedSeconds.value / duration) * 100);
});

function formatTime(seconds: number): string {
  const total = Math.max(0, Math.round(seconds));
  const minutes = Math.floor(total / 60);
  const secs = total % 60;
  return `${minutes}:${secs.toString().padStart(2, "0")}`;
}

const activeBoard = computed(
  () => boards.value.find((board) => board.id === activeBoardId.value) ?? null,
);

const hasBoardDestination = computed(() =>
  Boolean(
    activeBoard.value?.target_system_name ||
    activeBoard.value?.monitor_system_name,
  ),
);

// "Target" is what other people/apps hear — a virtual mic or a hardware
// input's underlying device. "Monitor" is a local output (e.g. the user's
// own speakers/headphones) so they can hear/test a clip without it going
// out to the target, or hear it at a different level than the target gets.
// These apply to every clip in the active tab — not per-clip (kept simple
// deliberately; per-tab granularity is as far as this goes for now).
const targetDeviceOptions = computed(() =>
  graph.value.devices.filter(
    (device) => device.direction === "input" || device.direction === "duplex",
  ),
);
const monitorDeviceOptions = computed(() =>
  graph.value.devices.filter(
    (device) => device.direction === "output" || device.direction === "duplex",
  ),
);

async function loadBoards() {
  loadingBoards.value = true;
  boards.value = await invoke<SoundboardBoard[]>("list_soundboard_boards");
  if (!boards.value.some((board) => board.id === activeBoardId.value)) {
    activeBoardId.value = boards.value[0]?.id ?? null;
  }
  loadingBoards.value = false;
}

async function loadClips() {
  clips.value = [];
  error.value = null;
  if (!activeBoardId.value) return;
  loadingClips.value = true;
  try {
    clips.value = await invoke<SoundboardClip[]>("list_soundboard_sounds", {
      boardId: activeBoardId.value,
    });
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loadingClips.value = false;
  }
}

async function playClip(clip: SoundboardClip) {
  if (!activeBoardId.value || playingClipId.value) return;
  playingClipId.value = clip.id;
  playingDurationSeconds.value = clip.duration_seconds;
  playingElapsedSeconds.value = 0;
  playingStartedAt = Date.now();
  clearProgressTimer();
  progressTimer = setInterval(() => {
    playingElapsedSeconds.value = (Date.now() - playingStartedAt) / 1000;
    const duration = playingDurationSeconds.value;
    if (duration !== null && playingElapsedSeconds.value >= duration) {
      resetPlaybackState();
    }
  }, PROGRESS_TICK_MS);

  try {
    await invoke("play_soundboard_clip", {
      boardId: activeBoardId.value,
      clipId: clip.id,
    });
  } catch (err) {
    resetPlaybackState();
    handleApplyResult(
      {
        success: false,
        message: err instanceof Error ? err.message : String(err),
      },
      "",
    );
  }
}

async function stopClip() {
  if (!playingClipId.value) return;
  try {
    await invoke("stop_soundboard_clip");
  } catch (err) {
    handleApplyResult(
      {
        success: false,
        message: err instanceof Error ? err.message : String(err),
      },
      "",
    );
  } finally {
    resetPlaybackState();
  }
}

function handleTileClick(clip: SoundboardClip) {
  if (clip.id === playingClipId.value) {
    stopClip();
  } else {
    playClip(clip);
  }
}

function selectBoard(boardId: string) {
  if (boardId === ADD_TAB_OPTION) {
    addBoard();
    return;
  }
  activeBoardId.value = boardId;
  loadClips();
}

const tabOptions = computed(() => [
  ...boards.value.map((board) => ({ value: board.id, label: board.name })),
  { value: ADD_TAB_OPTION, label: "+" },
]);

async function addBoard() {
  const name = await prompt({
    title: "New soundboard tab",
    message: 'Name this tab (e.g. "SFX", "Music")',
    placeholder: "SFX",
    confirmLabel: "Next: choose folder",
  });
  if (!name) return;

  const folder = await open({
    directory: true,
    multiple: false,
    title: `Folder for "${name}"`,
  });
  if (!folder || Array.isArray(folder)) return;

  const board: SoundboardBoard = {
    id: crypto.randomUUID(),
    name,
    folder,
    target_system_name: null,
    target_volume_percent: 100,
    monitor_system_name: null,
    monitor_volume_percent: 100,
  };
  try {
    await invoke("save_soundboard_board", { board });
    handleApplyResult({ success: true }, `Added "${name}" tab`);
    await loadBoards();
    selectBoard(board.id);
  } catch (err) {
    handleApplyResult(
      {
        success: false,
        message: err instanceof Error ? err.message : String(err),
      },
      "",
    );
  }
}

async function renameActiveBoard() {
  const board = activeBoard.value;
  if (!board) return;
  const name = await prompt({
    title: "Rename tab",
    message: "New name for this tab",
    defaultValue: board.name,
  });
  if (!name || name === board.name) return;

  try {
    await invoke("save_soundboard_board", { board: { ...board, name } });
    handleApplyResult({ success: true }, "Tab renamed");
    await loadBoards();
  } catch (err) {
    handleApplyResult(
      {
        success: false,
        message: err instanceof Error ? err.message : String(err),
      },
      "",
    );
  }
}

async function changeActiveBoardFolder() {
  const board = activeBoard.value;
  if (!board) return;
  const folder = await open({
    directory: true,
    multiple: false,
    title: `Folder for "${board.name}"`,
  });
  if (!folder || Array.isArray(folder)) return;

  try {
    await invoke("save_soundboard_board", { board: { ...board, folder } });
    handleApplyResult({ success: true }, "Folder updated");
    await loadBoards();
    await loadClips();
  } catch (err) {
    handleApplyResult(
      {
        success: false,
        message: err instanceof Error ? err.message : String(err),
      },
      "",
    );
  }
}

async function deleteActiveBoard() {
  const board = activeBoard.value;
  if (!board) return;
  const confirmed = await confirm(
    `Delete the "${board.name}" tab? Sound files on disk are not affected.`,
    {
      title: "Delete tab",
      confirmLabel: "Delete",
      cancelLabel: "Cancel",
    },
  );
  if (!confirmed) return;

  try {
    await invoke("delete_soundboard_board", { boardId: board.id });
    handleApplyResult({ success: true }, "Tab deleted");
    activeBoardId.value = null;
    await loadBoards();
    await loadClips();
  } catch (err) {
    handleApplyResult(
      {
        success: false,
        message: err instanceof Error ? err.message : String(err),
      },
      "",
    );
  }
}

// Local editable copies of the active board's destination fields — kept
// separate from `activeBoard` so the volume sliders can update their
// displayed number on every drag tick (`input`) without persisting on every
// tick too; persistence only happens on `change` (selection made / slider
// released).
const targetSystemName = ref<string | null>(null);
const targetVolume = ref(100);
const monitorSystemName = ref<string | null>(null);
const monitorVolume = ref(100);

watch(
  activeBoard,
  (board) => {
    targetSystemName.value = board?.target_system_name ?? null;
    targetVolume.value = board?.target_volume_percent ?? 100;
    monitorSystemName.value = board?.monitor_system_name ?? null;
    monitorVolume.value = board?.monitor_volume_percent ?? 100;
  },
  { immediate: true },
);

async function saveDestinations() {
  const board = activeBoard.value;
  if (!board) return;
  try {
    await invoke("save_soundboard_board", {
      board: {
        ...board,
        target_system_name: targetSystemName.value,
        target_volume_percent: targetVolume.value,
        monitor_system_name: monitorSystemName.value,
        monitor_volume_percent: monitorVolume.value,
      },
    });
    await loadBoards();
  } catch (err) {
    handleApplyResult(
      {
        success: false,
        message: err instanceof Error ? err.message : String(err),
      },
      "",
    );
  }
}

onMounted(async () => {
  await loadBoards();
  await loadClips();
});
</script>

<template>
  <div class="soundboard-view">
    <header class="soundboard-header view-header">
      <div>
        <p class="eyebrow">Soundboard</p>
      </div>
    </header>

    <p v-if="loadingBoards" class="status">Loading tabs…</p>

    <template v-else-if="boards.length === 0">
      <div class="soundboard-empty-state">
        <strong>No soundboard tabs yet.</strong>
        <p>Add a tab and point it at a folder of sound files to get started.</p>
      </div>
      <div class="view-actions">
        <button type="button" @click="addBoard">+ Add tab</button>
      </div>
    </template>

    <template v-else>
      <SegmentedControl
        :model-value="activeBoardId ?? ''"
        :options="tabOptions"
        @update:model-value="selectBoard"
      />

      <div v-if="activeBoard" class="soundboard-board-toolbar">
        <span class="soundboard-board-folder" :title="activeBoard.folder">{{
          activeBoard.folder
        }}</span>
        <div class="view-actions">
          <button type="button" :disabled="loadingClips" @click="loadClips">
            Refresh
          </button>
          <button type="button" @click="changeActiveBoardFolder">
            Change folder
          </button>
          <button type="button" @click="renameActiveBoard">Rename</button>
          <button type="button" @click="deleteActiveBoard">Delete tab</button>
        </div>
      </div>

      <div v-if="activeBoard" class="soundboard-destinations">
        <div class="soundboard-destination">
          <label
            class="soundboard-destination-label"
            for="soundboard-target-device"
          >
            Target (others hear this)
          </label>
          <select
            id="soundboard-target-device"
            v-model="targetSystemName"
            @change="saveDestinations"
          >
            <option :value="null">None</option>
            <option
              v-for="device in targetDeviceOptions"
              :key="device.id"
              :value="device.system_name"
            >
              {{ device.label }}
            </option>
          </select>
          <div class="soundboard-destination-volume-row">
            <input
              v-model.number="targetVolume"
              type="range"
              min="0"
              max="100"
              :disabled="!targetSystemName"
              @change="saveDestinations"
            />
            <span class="soundboard-destination-volume-value"
              >{{ targetVolume }}%</span
            >
          </div>
        </div>

        <div class="soundboard-destination">
          <label
            class="soundboard-destination-label"
            for="soundboard-monitor-device"
          >
            Monitor (you hear this)
          </label>
          <select
            id="soundboard-monitor-device"
            v-model="monitorSystemName"
            @change="saveDestinations"
          >
            <option :value="null">None</option>
            <option
              v-for="device in monitorDeviceOptions"
              :key="device.id"
              :value="device.system_name"
            >
              {{ device.label }}
            </option>
          </select>
          <div class="soundboard-destination-volume-row">
            <input
              v-model.number="monitorVolume"
              type="range"
              min="0"
              max="100"
              :disabled="!monitorSystemName"
              @change="saveDestinations"
            />
            <span class="soundboard-destination-volume-value"
              >{{ monitorVolume }}%</span
            >
          </div>
        </div>
      </div>

      <div
        v-if="activeBoard && clips.length > 0"
        class="soundboard-layout-toolbar"
      >
        <SegmentedControl
          :model-value="cardSize"
          :options="cardSizeOptions"
          :disabled="clipLayout !== 'cards'"
          @update:model-value="selectCardSize"
        />
        <SegmentedControl
          :model-value="clipLayout"
          :options="layoutOptions"
          @update:model-value="selectClipLayout"
        />
      </div>

      <p v-if="loadingClips" class="status">Loading clips…</p>
      <p v-else-if="error" class="status error">{{ error }}</p>

      <div v-else-if="clips.length === 0" class="soundboard-empty-state">
        <strong>No supported sound files found.</strong>
        <p v-if="activeBoard">
          Add wav, flac, ogg, mp3, aiff, m4a, or opus files to "{{
            activeBoard.folder
          }}".
        </p>
      </div>

      <div
        v-else
        class="soundboard-clips"
        :class="
          clipLayout === 'list'
            ? 'soundboard-list'
            : `soundboard-grid soundboard-grid--${cardSize}`
        "
      >
        <button
          v-for="clip in clips"
          :key="clip.id"
          type="button"
          class="soundboard-tile"
          :class="{
            playing: playingClipId === clip.id,
            'no-target': !hasBoardDestination,
            'soundboard-tile--list': clipLayout === 'list',
          }"
          :disabled="playingClipId !== null && playingClipId !== clip.id"
          :title="
            playingClipId === clip.id
              ? `${clip.label} — click to stop`
              : hasBoardDestination
                ? clip.label
                : `${clip.label} — this tab has no target or monitor device set yet`
          "
          @click="handleTileClick(clip)"
        >
          <span class="soundboard-tile-icon">{{
            playingClipId === clip.id ? "⏹" : "🔊"
          }}</span>
          <span class="soundboard-tile-label">{{ clip.label }}</span>
          <div
            v-if="playingClipId === clip.id"
            class="soundboard-tile-progress"
          >
            <div class="soundboard-tile-progress-bar">
              <div
                class="soundboard-tile-progress-fill"
                :style="{ width: `${playingProgressPercent}%` }"
              />
            </div>
            <div class="soundboard-tile-progress-times">
              <span>{{ formatTime(playingElapsedSeconds) }}</span>
              <span v-if="playingDurationSeconds !== null">{{
                formatTime(playingDurationSeconds)
              }}</span>
            </div>
          </div>
        </button>
      </div>
    </template>
  </div>
</template>
