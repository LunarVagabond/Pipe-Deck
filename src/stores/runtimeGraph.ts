import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { onMounted, onUnmounted, ref } from "vue";
import type { AppConfig, ProfileIndexEntry, RuntimeGraph } from "../types/graph";

const emptyGraph = (): RuntimeGraph => ({
  devices: [],
  streams: [],
  links: [],
});

export function useRuntimeGraph() {
  const graph = ref<RuntimeGraph>(emptyGraph());
  const loading = ref(true);
  const error = ref<string | null>(null);
  let unlisten: (() => void) | null = null;

  async function refresh() {
    loading.value = true;
    error.value = null;

    try {
      graph.value = await invoke<RuntimeGraph>("get_runtime_graph");
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err);
    } finally {
      loading.value = false;
    }
  }

  onMounted(async () => {
    await refresh();
    unlisten = await listen<RuntimeGraph>("graph-updated", (event) => {
      graph.value = event.payload;
      loading.value = false;
      error.value = null;
    });
  });

  onUnmounted(() => {
    unlisten?.();
  });

  return { graph, loading, error, refresh };
}

export function useAppConfig() {
  const config = ref<AppConfig | null>(null);
  const profiles = ref<ProfileIndexEntry[]>([]);

  onMounted(async () => {
    try {
      config.value = await invoke<AppConfig>("get_config");
      profiles.value = await invoke<ProfileIndexEntry[]>("list_profiles");
    } catch {
      config.value = { version: 1, profile_index: [], preferences: {} };
      profiles.value = [];
    }
  });

  return { config, profiles };
}
