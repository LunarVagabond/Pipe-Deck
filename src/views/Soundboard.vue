<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useApplyResult } from "../stores/notices";
import type { SoundboardClip } from "../types/graph";

// #397 (play button wiring) and #398 (per-sound target-device picker) land
// in later tickets — clips are listed here, not yet playable or assignable.
const { handleApplyResult } = useApplyResult();

const folder = ref("");
const folderInput = ref("");
const clips = ref<SoundboardClip[]>([]);
const loading = ref(true);
const error = ref<string | null>(null);
const saving = ref(false);

async function loadFolder() {
  const value = await invoke<string | null>("get_soundboard_folder");
  folder.value = value ?? "";
  folderInput.value = folder.value;
}

async function loadClips() {
  loading.value = true;
  error.value = null;
  clips.value = [];
  if (!folder.value) {
    loading.value = false;
    return;
  }
  try {
    clips.value = await invoke<SoundboardClip[]>("list_soundboard_sounds");
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
}

async function saveFolder() {
  saving.value = true;
  try {
    await invoke("set_soundboard_folder", { folder: folderInput.value });
    handleApplyResult({ success: true }, "Soundboard folder saved");
    await loadFolder();
    await loadClips();
  } catch (err) {
    handleApplyResult(
      { success: false, message: err instanceof Error ? err.message : String(err) },
      "",
    );
  } finally {
    saving.value = false;
  }
}

onMounted(async () => {
  await loadFolder();
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

    <form class="soundboard-folder-form" @submit.prevent="saveFolder">
      <label class="soundboard-folder-label" for="soundboard-folder-input">Sound clips folder</label>
      <div class="soundboard-folder-row">
        <input
          id="soundboard-folder-input"
          v-model="folderInput"
          type="text"
          placeholder="/home/you/Sounds"
          autocomplete="off"
        />
        <button type="submit" :disabled="saving">Save</button>
      </div>
    </form>

    <p v-if="loading" class="status">Loading clips…</p>
    <p v-else-if="error" class="status error">{{ error }}</p>

    <div v-else-if="!folder" class="soundboard-empty-state">
      <strong>No sound clips configured yet.</strong>
      <p>Point Pipe Deck at a folder of sound files above to get started.</p>
    </div>

    <div v-else-if="clips.length === 0" class="soundboard-empty-state">
      <strong>No supported sound files found.</strong>
      <p>Add wav, flac, ogg, mp3, aiff, m4a, or opus files to "{{ folder }}".</p>
    </div>

    <div v-else class="soundboard-grid">
      <article v-for="clip in clips" :key="clip.id" class="soundboard-tile">
        <span class="soundboard-tile-icon">🔊</span>
        <span class="soundboard-tile-label">{{ clip.label }}</span>
      </article>
    </div>
  </div>
</template>
