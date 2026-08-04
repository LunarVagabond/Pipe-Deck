<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import SegmentedControl from "../components/SegmentedControl.vue";
import { useApplyResult } from "../stores/notices";
import { usePrompt } from "../stores/prompt";
import { useConfirm } from "../stores/confirm";
import { useRuntimeGraph } from "../stores/runtimeGraph";
import type { SoundboardBoard, SoundboardClip } from "../types/graph";

const ADD_TAB_OPTION = "__add_tab__";

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

const activeBoard = computed(() => boards.value.find((board) => board.id === activeBoardId.value) ?? null);

const hasBoardDestination = computed(
  () => Boolean(activeBoard.value?.target_system_name || activeBoard.value?.monitor_system_name),
);

// "Target" is what other people/apps hear — a virtual mic or a hardware
// input's underlying device. "Monitor" is a local output (e.g. the user's
// own speakers/headphones) so they can hear/test a clip without it going
// out to the target, or hear it at a different level than the target gets.
// These apply to every clip in the active tab — not per-clip (kept simple
// deliberately; per-tab granularity is as far as this goes for now).
const targetDeviceOptions = computed(() =>
  graph.value.devices.filter((device) => device.direction === "input" || device.direction === "duplex"),
);
const monitorDeviceOptions = computed(() =>
  graph.value.devices.filter((device) => device.direction === "output" || device.direction === "duplex"),
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
    clips.value = await invoke<SoundboardClip[]>("list_soundboard_sounds", { boardId: activeBoardId.value });
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loadingClips.value = false;
  }
}

async function playClip(clip: SoundboardClip) {
  if (!activeBoardId.value || playingClipId.value) return;
  playingClipId.value = clip.id;
  try {
    await invoke("play_soundboard_clip", { boardId: activeBoardId.value, clipId: clip.id });
  } catch (err) {
    handleApplyResult(
      { success: false, message: err instanceof Error ? err.message : String(err) },
      "",
    );
  } finally {
    playingClipId.value = null;
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
    message: "Name this tab (e.g. \"SFX\", \"Music\")",
    placeholder: "SFX",
    confirmLabel: "Next: choose folder",
  });
  if (!name) return;

  const folder = await open({ directory: true, multiple: false, title: `Folder for "${name}"` });
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
      { success: false, message: err instanceof Error ? err.message : String(err) },
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
      { success: false, message: err instanceof Error ? err.message : String(err) },
      "",
    );
  }
}

async function changeActiveBoardFolder() {
  const board = activeBoard.value;
  if (!board) return;
  const folder = await open({ directory: true, multiple: false, title: `Folder for "${board.name}"` });
  if (!folder || Array.isArray(folder)) return;

  try {
    await invoke("save_soundboard_board", { board: { ...board, folder } });
    handleApplyResult({ success: true }, "Folder updated");
    await loadBoards();
    await loadClips();
  } catch (err) {
    handleApplyResult(
      { success: false, message: err instanceof Error ? err.message : String(err) },
      "",
    );
  }
}

async function deleteActiveBoard() {
  const board = activeBoard.value;
  if (!board) return;
  const confirmed = await confirm(`Delete the "${board.name}" tab? Sound files on disk are not affected.`, {
    title: "Delete tab",
    confirmLabel: "Delete",
    cancelLabel: "Cancel",
  });
  if (!confirmed) return;

  try {
    await invoke("delete_soundboard_board", { boardId: board.id });
    handleApplyResult({ success: true }, "Tab deleted");
    activeBoardId.value = null;
    await loadBoards();
    await loadClips();
  } catch (err) {
    handleApplyResult(
      { success: false, message: err instanceof Error ? err.message : String(err) },
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
      { success: false, message: err instanceof Error ? err.message : String(err) },
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
        <span class="soundboard-board-folder" :title="activeBoard.folder">{{ activeBoard.folder }}</span>
        <div class="view-actions">
          <button type="button" :disabled="loadingClips" @click="loadClips">Refresh</button>
          <button type="button" @click="changeActiveBoardFolder">Change folder</button>
          <button type="button" @click="renameActiveBoard">Rename</button>
          <button type="button" @click="deleteActiveBoard">Delete tab</button>
        </div>
      </div>

      <div v-if="activeBoard" class="soundboard-destinations">
        <div class="soundboard-destination">
          <label class="soundboard-destination-label" for="soundboard-target-device">
            Target (others hear this)
          </label>
          <select id="soundboard-target-device" v-model="targetSystemName" @change="saveDestinations">
            <option :value="null">None</option>
            <option v-for="device in targetDeviceOptions" :key="device.id" :value="device.system_name">
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
            <span class="soundboard-destination-volume-value">{{ targetVolume }}%</span>
          </div>
        </div>

        <div class="soundboard-destination">
          <label class="soundboard-destination-label" for="soundboard-monitor-device">
            Monitor (you hear this)
          </label>
          <select id="soundboard-monitor-device" v-model="monitorSystemName" @change="saveDestinations">
            <option :value="null">None</option>
            <option v-for="device in monitorDeviceOptions" :key="device.id" :value="device.system_name">
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
            <span class="soundboard-destination-volume-value">{{ monitorVolume }}%</span>
          </div>
        </div>
      </div>

      <p v-if="loadingClips" class="status">Loading clips…</p>
      <p v-else-if="error" class="status error">{{ error }}</p>

      <div v-else-if="clips.length === 0" class="soundboard-empty-state">
        <strong>No supported sound files found.</strong>
        <p v-if="activeBoard">Add wav, flac, ogg, mp3, aiff, m4a, or opus files to "{{ activeBoard.folder }}".</p>
      </div>

      <div v-else class="soundboard-grid">
        <button
          v-for="clip in clips"
          :key="clip.id"
          type="button"
          class="soundboard-tile"
          :class="{ playing: playingClipId === clip.id, 'no-target': !hasBoardDestination }"
          :disabled="playingClipId !== null"
          :title="hasBoardDestination ? clip.label : `${clip.label} — this tab has no target or monitor device set yet`"
          @click="playClip(clip)"
        >
          <span class="soundboard-tile-icon">🔊</span>
          <span class="soundboard-tile-label">{{ clip.label }}</span>
        </button>
      </div>
    </template>
  </div>
</template>
