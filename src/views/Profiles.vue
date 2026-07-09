<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useApplyResult } from "../stores/notices";
import { useProfiles } from "../stores/profiles";
import type { Profile, RoutingDrift } from "../types/graph";

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
const activeDrift = ref<RoutingDrift | null>(null);
const saveAsName = ref("");
const showSaveAs = ref(false);
const importPath = ref("");
const exportPath = ref("");
const swapConfirmId = ref<string | null>(null);

const activeName = computed(() => {
  const active = activeProfileId.value;
  return profiles.value.find((profile) => profile.id === active)?.name ?? active ?? "None";
});

async function loadDrift(profileId?: string) {
  if (!profileId) {
    activeDrift.value = null;
    return;
  }
  try {
    activeDrift.value = await invoke<RoutingDrift>("get_profile_drift", { profileId });
  } catch {
    activeDrift.value = null;
  }
}

watch(activeProfileId, (profileId) => {
  void loadDrift(profileId);
}, { immediate: true });

onMounted(async () => {
  await listen("graph-updated", () => {
    void loadDrift(activeProfileId.value);
  });
});

async function loadDetails(profileId: string) {
  selectedProfile.value = await getProfile(profileId);
}

async function onSave(profileId: string) {
  await saveProfile(profileId);
  await refresh();
  await loadDetails(profileId);
  await loadDrift(profileId);
  handleApplyResult({ success: true }, "Profile saved from live routing");
}

async function onSaveAs() {
  if (!saveAsName.value.trim()) return;
  const id = saveAsName.value.trim().toLowerCase().replace(/\s+/g, "-");
  await saveProfileAs(id, saveAsName.value.trim());
  saveAsName.value = "";
  showSaveAs.value = false;
  await refresh();
}

async function onApply(profileId: string) {
  const result = await invoke<{ success: boolean; message?: string }>("apply_profile_routes", {
    profileId,
  });
  handleApplyResult(result, "Profile routes applied to PipeWire");
  await loadDrift(profileId);
}

async function onApplyAll(profileId: string) {
  if (swapConfirmId.value !== profileId) {
    swapConfirmId.value = profileId;
    return;
  }
  const result = await swapProfile(profileId);
  handleApplyResult(result, "Full profile applied");
  swapConfirmId.value = null;
  await refresh();
  await loadDetails(profileId);
  await loadDrift(profileId);
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
        <p class="eyebrow">Desired routing</p>
        <h1>Profiles</h1>
      </div>
      <div class="profiles-actions">
        <button
          v-if="activeProfileId"
          type="button"
          @click="onSave(activeProfileId)"
        >
          Save
        </button>
        <button
          v-if="activeProfileId"
          type="button"
          class="primary"
          @click="onApply(activeProfileId)"
        >
          Apply
        </button>
        <button type="button" @click="showSaveAs = !showSaveAs">Save as…</button>
        <button type="button" @click="refresh">Refresh</button>
      </div>
    </header>

    <p class="profiles-active">Active profile: <strong>{{ activeName }}</strong></p>
    <p class="profiles-help">
      <strong>Save</strong> writes the dashboard’s live routing into the profile file — it does not change PipeWire.
      <strong>Apply</strong> pushes the profile’s saved routes to PipeWire — it does not change the profile file.
      Edit routing on the dashboard first, then Save; or edit a profile’s wants below, then Apply.
    </p>

    <section v-if="activeDrift?.has_drift" class="profile-drift">
      <div class="profile-drift-header">
        <h2>Live routing differs from {{ activeDrift.profile_name }}</h2>
        <button
          v-if="activeProfileId"
          type="button"
          class="primary"
          @click="onApply(activeProfileId)"
        >
          Apply
        </button>
      </div>
      <ul class="profile-drift-list">
        <li v-for="item in activeDrift.items" :key="item.stream_id">
          <strong>{{ item.stream_label }}</strong>
          <span class="profile-drift-arrow">
            {{ item.live_target_label ?? "Unrouted" }}
            →
            {{ item.desired_target_label ?? "Unspecified" }}
          </span>
        </li>
      </ul>
    </section>

    <section v-else-if="activeDrift && activeProfileId" class="profile-drift in-sync">
      <p>Live routing matches <strong>{{ activeDrift.profile_name }}</strong>.</p>
    </section>

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
          <button type="button" @click="loadDetails(profile.id)">Wants</button>
          <button
            v-if="profile.id === activeProfileId"
            type="button"
            @click="onSave(profile.id)"
          >
            Save
          </button>
          <button type="button" class="primary" @click="onApply(profile.id)">Apply</button>
          <button type="button" @click="onApplyAll(profile.id)">
            {{ swapConfirmId === profile.id ? "Confirm apply all" : "Apply all" }}
          </button>
        </div>
        <p v-if="swapConfirmId === profile.id" class="profile-meta">
          Apply all also restores virtual devices and volumes.
        </p>
        <div class="profiles-form compact">
          <input v-model="exportPath" type="text" placeholder="Export .tar.gz path" />
          <button type="button" @click="onExport(profile.id)">Export</button>
        </div>
      </article>
    </div>

    <section v-if="selectedProfile" class="profile-details">
      <h3>{{ selectedProfile.name }} wants</h3>
      <p class="profile-meta">
        Updated {{ selectedProfile.updated }} · {{ selectedProfile.routing_intents.length }} routes
      </p>
      <ul>
        <li v-for="intent in selectedProfile.routing_intents" :key="intent.stream_id">
          {{ intent.stream_id }} → {{ intent.target_device_id ?? intent.target_device_ids?.[0] ?? "—" }}
        </li>
      </ul>
    </section>
  </div>
</template>
