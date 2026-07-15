import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { onMounted, onUnmounted, ref } from "vue";
import { createTrailingDebouncer } from "../composables/useThrottledGraphUpdates";
import type { AppConfig, ProfileIndexEntry, RuntimeGraph } from "../types/graph";

// Backend already coalesces PipeWire monitor events before emitting
// "graph-updated" (up to ~2-5Hz, see live.rs's MONITOR_DEBOUNCE/
// MAX_COALESCE_WINDOW), but applies every push unconditionally on the
// frontend. This bounds the resulting Vue reactivity/Vue Flow rebuild rate
// under sustained churn while staying well under the "reflects within
// 500ms of a user action" budget (see docs/PipeWire_Design.md).
const GRAPH_UPDATE_DEBOUNCE_MS = 100;
const GRAPH_UPDATE_MAX_WAIT_MS = 150;

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

  const scheduleGraphUpdate = createTrailingDebouncer<RuntimeGraph>(
    (payload) => {
      graph.value = payload;
      loading.value = false;
      error.value = null;
    },
    { wait: GRAPH_UPDATE_DEBOUNCE_MS, maxWait: GRAPH_UPDATE_MAX_WAIT_MS },
  );

  onMounted(async () => {
    await refresh();
    unlisten = await listen<RuntimeGraph>("graph-updated", (event) => {
      scheduleGraphUpdate(event.payload);
    });
  });

  onUnmounted(() => {
    unlisten?.();
    scheduleGraphUpdate.cancel();
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
