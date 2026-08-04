<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import SegmentedControl from "../components/SegmentedControl.vue";
import { useApplyResult } from "../stores/notices";
import { usePrompt } from "../stores/prompt";
import { useConfirm } from "../stores/confirm";
import type { SoundboardBoard, SoundboardClip } from "../types/graph";

const ADD_TAB_OPTION = "__add_tab__";

// #398 (per-sound target-device picker) lands in a later ticket — a clip
// with no target assigned yet is playable in the sense that the click is
// wired up, but the backend rejects it (no UI to assign one exists before
// #398, only direct config.yaml edits).
const { handleApplyResult } = useApplyResult();
const { prompt } = usePrompt();
const { confirm } = useConfirm();

const boards = ref<SoundboardBoard[]>([]);
const activeBoardId = ref<string | null>(null);
const clips = ref<SoundboardClip[]>([]);
const loadingBoards = ref(true);
const loadingClips = ref(false);
const error = ref<string | null>(null);
const playingClipId = ref<string | null>(null);

const activeBoard = computed(() => boards.value.find((board) => board.id === activeBoardId.value) ?? null);

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

function hasTarget(clip: SoundboardClip): boolean {
  return Boolean(activeBoard.value?.clip_targets[clip.id]);
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

  const board: SoundboardBoard = { id: crypto.randomUUID(), name, folder, clip_targets: {} };
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
          :class="{ playing: playingClipId === clip.id, 'no-target': !hasTarget(clip) }"
          :disabled="playingClipId !== null"
          :title="hasTarget(clip) ? clip.label : `${clip.label} — no target device set yet`"
          @click="playClip(clip)"
        >
          <span class="soundboard-tile-icon">🔊</span>
          <span class="soundboard-tile-label">{{ clip.label }}</span>
        </button>
      </div>
    </template>
  </div>
</template>
