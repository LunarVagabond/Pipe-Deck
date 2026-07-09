<script setup lang="ts">
import { computed, ref } from "vue";
import { useApplyResult } from "../stores/notices";
import { useProfiles } from "../stores/profiles";
import type { Profile } from "../types/graph";

const {
  profiles,
  activeProfileId,
  loading,
  error,
  refresh,
  getProfile,
  saveProfile,
  saveProfileAs,
  swapProfile,
  importProfile,
  exportProfile,
} = useProfiles();

const { handleApplyResult } = useApplyResult();

const selectedProfile = ref<Profile | null>(null);
const saveAsName = ref("");
const showSaveAs = ref(false);
const importPath = ref("");
const exportPath = ref("");
const swapConfirmId = ref<string | null>(null);

const activeName = computed(() => {
  const active = activeProfileId.value;
  return profiles.value.find((profile) => profile.id === active)?.name ?? active ?? "None";
});

async function loadDetails(profileId: string) {
  selectedProfile.value = await getProfile(profileId);
}

async function onSaveActive() {
  if (!activeProfileId.value) return;
  await saveProfile(activeProfileId.value);
  await refresh();
  await loadDetails(activeProfileId.value);
}

async function onSaveAs() {
  if (!saveAsName.value.trim()) return;
  const id = saveAsName.value.trim().toLowerCase().replace(/\s+/g, "-");
  await saveProfileAs(id, saveAsName.value.trim());
  saveAsName.value = "";
  showSaveAs.value = false;
  await refresh();
}

async function onSwap(profileId: string) {
  if (swapConfirmId.value !== profileId) {
    swapConfirmId.value = profileId;
    return;
  }
  const result = await swapProfile(profileId);
  handleApplyResult(result, "Profile applied");
  swapConfirmId.value = null;
  await refresh();
  await loadDetails(profileId);
}

async function onImport() {
  if (!importPath.value.trim()) return;
  await importProfile(importPath.value.trim());
  importPath.value = "";
  await refresh();
}

async function onExport(profileId: string) {
  if (!exportPath.value.trim()) return;
  await exportProfile(profileId, exportPath.value.trim());
  exportPath.value = "";
}
</script>

<template>
  <div class="profiles-view">
    <header class="profiles-header">
      <div>
        <p class="eyebrow">Saved setups</p>
        <h1>Profiles</h1>
      </div>
      <div class="profiles-actions">
        <button type="button" @click="onSaveActive">Save current</button>
        <button type="button" @click="showSaveAs = !showSaveAs">Save as…</button>
        <button type="button" @click="refresh">Refresh</button>
      </div>
    </header>

    <p class="profiles-active">Active profile: <strong>{{ activeName }}</strong></p>

    <div v-if="showSaveAs" class="profiles-form">
      <input v-model="saveAsName" type="text" placeholder="New profile name" />
      <button type="button" @click="onSaveAs">Create</button>
    </div>

    <div class="profiles-form">
      <input v-model="importPath" type="text" placeholder="Import profile YAML path" />
      <button type="button" @click="onImport">Import</button>
    </div>

    <p v-if="loading" class="status">Loading profiles…</p>
    <p v-else-if="error" class="status error">{{ error }}</p>

    <div v-else class="profile-list">
      <article
        v-for="profile in profiles"
        :key="profile.id"
        class="profile-card"
        :class="{ active: profile.id === activeProfileId }"
      >
        <div class="profile-card-header">
          <h2>{{ profile.name }}</h2>
          <span v-if="profile.id === activeProfileId" class="profile-badge">Active</span>
        </div>
        <p class="profile-meta">{{ profile.file }}</p>
        <div class="profile-card-actions">
          <button type="button" @click="loadDetails(profile.id)">Details</button>
          <button
            type="button"
            class="primary"
            @click="onSwap(profile.id)"
          >
            {{ swapConfirmId === profile.id ? "Confirm swap" : "Swap to" }}
          </button>
        </div>
        <div class="profiles-form compact">
          <input v-model="exportPath" type="text" placeholder="Export .tar.gz path" />
          <button type="button" @click="onExport(profile.id)">Export</button>
        </div>
      </article>
    </div>

    <section v-if="selectedProfile" class="profile-details">
      <h3>{{ selectedProfile.name }}</h3>
      <p class="profile-meta">
        Updated {{ selectedProfile.updated }} · {{ selectedProfile.routing_intents.length }} routes
      </p>
      <ul>
        <li v-for="intent in selectedProfile.routing_intents" :key="intent.stream_id">
          {{ intent.stream_id }} → {{ intent.target_device_id }}
        </li>
      </ul>
    </section>
  </div>
</template>
