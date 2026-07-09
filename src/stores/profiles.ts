import { invoke } from "@tauri-apps/api/core";
import { onMounted, ref } from "vue";
import type { Profile, ProfileIndexEntry } from "../types/graph";

export function useProfiles() {
  const profiles = ref<ProfileIndexEntry[]>([]);
  const activeProfileId = ref<string | undefined>();
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function refresh() {
    loading.value = true;
    error.value = null;
    try {
      const [list, config] = await Promise.all([
        invoke<ProfileIndexEntry[]>("list_profiles"),
        invoke<{ active_profile?: string }>("get_config"),
      ]);
      profiles.value = list;
      activeProfileId.value = config.active_profile;
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err);
    } finally {
      loading.value = false;
    }
  }

  async function getProfile(profileId: string) {
    return invoke<Profile>("get_profile", { profileId });
  }

  async function saveProfile(profileId: string, name?: string) {
    return invoke<Profile>("save_profile", { profileId, name: name ?? null });
  }

  async function saveProfileAs(profileId: string, name: string) {
    return invoke<Profile>("save_profile_as", { profileId, name });
  }

  async function swapProfile(profileId: string) {
    return invoke<{ success: boolean; message?: string }>("swap_profile", { profileId });
  }

  async function importProfile(sourcePath: string) {
    return invoke<ProfileIndexEntry>("import_profile", { sourcePath });
  }

  async function importProfileArchive(sourcePath: string) {
    return invoke<ProfileIndexEntry>("import_profile_archive", { sourcePath });
  }

  async function exportProfile(profileId: string, destination: string) {
    return invoke("export_profile", { profileId, destination });
  }

  onMounted(refresh);

  return {
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
    importProfileArchive,
    exportProfile,
  };
}
