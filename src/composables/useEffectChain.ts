import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { onMounted, onUnmounted, ref } from "vue";
import { useApplyResult, useNotices } from "../stores/notices";
import {
  emptyDynamicsStage,
  emptyEq5BandStage,
  type EffectChainConfig,
  type EffectStage,
  type FxCapabilities,
} from "../types/graph";

/** PD-025: node-scoped effects UI — one non-blocking toast per session, the
 * first time any device gets a stage added, instead of a confirm dialog. */
let hasShownRestartToast = false;

function emptyChain(): EffectChainConfig {
  return {
    stages: [],
    compressor: emptyDynamicsStage(),
    limiter: emptyDynamicsStage(),
    noise_gate: emptyDynamicsStage(),
    bypassed: false,
  };
}

/**
 * Shared effect-chain state/actions for the Routing graph node, Mixer strip,
 * and Effects page — each surface calls this independently (matching
 * `useRuntimeGraph`'s pattern: no hand-rolled singleton, every mounted
 * instance fetches its own copy and reacts to the same backend
 * `graph-updated` push so they all stay in sync after any one of them
 * mutates a chain).
 */
export function useEffectChain() {
  const { handleApplyResult } = useApplyResult();
  const { pushNotice } = useNotices();
  const chains = ref<Record<string, EffectChainConfig>>({});
  const capabilities = ref<FxCapabilities>({ builtin_eq: false, builtin_gain: false, builtin_limiter: false });
  const loading = ref(true);
  let unlisten: (() => void) | null = null;
  const liveParamsTimers: Record<string, number> = {};
  /** Devices with a live-params write still in flight — `graph-updated`
   * fires on every graph change (a brand new stream starting is enough),
   * not just effects changes, and each firing triggers `refresh()`. Without
   * this guard, `refresh()` blindly overwrites `chains.value` from disk,
   * which can still hold the pre-write value if the fetch lands before the
   * in-flight write's `ConfigStore` persist does — stomping the optimistic
   * edit and reading to the user as "my change/effects got reset". */
  const pendingWrites: Record<string, boolean> = {};

  function chainFor(deviceId: string): EffectChainConfig {
    return chains.value[deviceId] ?? emptyChain();
  }

  async function refresh() {
    try {
      const fetched = await invoke<Record<string, EffectChainConfig>>("get_effect_chains");
      const merged = { ...fetched };
      for (const deviceId of Object.keys(pendingWrites)) {
        if (pendingWrites[deviceId] && chains.value[deviceId]) {
          merged[deviceId] = chains.value[deviceId];
        }
      }
      chains.value = merged;
    } catch {
      chains.value = {};
    } finally {
      loading.value = false;
    }
  }

  async function refreshCapabilities() {
    try {
      capabilities.value = await invoke<FxCapabilities>("get_effect_capabilities");
    } catch {
      capabilities.value = { builtin_eq: false, builtin_gain: false, builtin_limiter: false };
    }
  }

  function maybeShowRestartToast() {
    if (hasShownRestartToast) return;
    hasShownRestartToast = true;
    pushNotice("info", "Adding an effect briefly restarts Pipe Deck's effects daemon.");
  }

  /** Adds a new EQ stage with a freshly generated id and applies immediately
   * — no separate "enable live effects" step (PD-025). */
  async function addEq5BandStage(deviceId: string) {
    const stage = emptyEq5BandStage(crypto.randomUUID());
    maybeShowRestartToast();
    try {
      await invoke("add_effect_stage", { deviceId, stage });
      await refresh();
    } catch (error) {
      handleApplyResult(
        { success: false, message: error instanceof Error ? error.message : String(error) },
        "",
      );
    }
  }

  async function removeStage(deviceId: string, stageId: string) {
    try {
      await invoke("remove_effect_stage", { deviceId, stageId });
      await refresh();
    } catch (error) {
      handleApplyResult(
        { success: false, message: error instanceof Error ? error.message : String(error) },
        "",
      );
    }
  }

  async function reorderStages(deviceId: string, orderedStageIds: string[]) {
    // Optimistic local reorder so the UI doesn't visibly snap back while the
    // apply round-trips.
    const chain = chains.value[deviceId];
    if (chain) {
      const byId = new Map(chain.stages.map((stage) => [stage.id, stage]));
      chains.value = {
        ...chains.value,
        [deviceId]: {
          ...chain,
          stages: orderedStageIds.map((id) => byId.get(id)).filter((stage): stage is EffectStage => Boolean(stage)),
        },
      };
    }
    try {
      await invoke("reorder_effect_stages", { deviceId, orderedStageIds });
      await refresh();
    } catch (error) {
      handleApplyResult(
        { success: false, message: error instanceof Error ? error.message : String(error) },
        "",
      );
      await refresh();
    }
  }

  /** Pushes a live-params write for `deviceId`, marking it pending so a
   * `graph-updated`-triggered `refresh()` racing ahead of this write's
   * `ConfigStore` persist can't stomp `chains.value` back to the pre-write
   * state in the meantime. */
  async function pushLiveParams(deviceId: string, config: EffectChainConfig) {
    pendingWrites[deviceId] = true;
    try {
      await invoke("set_effect_chain_live_params", { deviceId, config });
    } catch (error) {
      handleApplyResult(
        { success: false, message: error instanceof Error ? error.message : String(error) },
        "",
      );
    } finally {
      delete pendingWrites[deviceId];
    }
  }

  /** Live param push for one stage's sliders — debounced, no restart, safe
   * on every drag tick once the stage is already live (which it always is
   * now, per PD-025). Only the single most recent timer per device is ever
   * live, so a burst of ticks collapses into one in-flight write instead of
   * several racing requests that could settle out of order. */
  function scheduleStageUpdate(deviceId: string, updatedStage: EffectStage) {
    const chain = chains.value[deviceId] ?? emptyChain();
    const nextChain: EffectChainConfig = {
      ...chain,
      stages: chain.stages.map((stage) => (stage.id === updatedStage.id ? updatedStage : stage)),
    };
    chains.value = { ...chains.value, [deviceId]: nextChain };

    window.clearTimeout(liveParamsTimers[deviceId]);
    liveParamsTimers[deviceId] = window.setTimeout(() => {
      void pushLiveParams(deviceId, nextChain);
    }, 60);
  }

  /** Toggles bypass for the whole chain — a live param when the device
   * already has a stage (immediate, no restart), otherwise just persisted
   * (nothing live to bypass yet). Debounced through the same per-device
   * timer `scheduleStageUpdate` uses: clicking the toggle repeatedly and
   * quickly used to fire one un-sequenced `invoke` per click, and whichever
   * happened to resolve last — not whichever was clicked last — won,
   * reading as "the toggle only works sometimes". Collapsing to a single
   * in-flight write per device fixes that the same way it already does for
   * slider drags. */
  function setBypassed(deviceId: string, bypassed: boolean) {
    const chain = chains.value[deviceId] ?? emptyChain();
    const nextChain: EffectChainConfig = { ...chain, bypassed };
    chains.value = { ...chains.value, [deviceId]: nextChain };

    window.clearTimeout(liveParamsTimers[deviceId]);
    if (nextChain.stages.length > 0) {
      liveParamsTimers[deviceId] = window.setTimeout(() => {
        void pushLiveParams(deviceId, nextChain);
      }, 60);
      return;
    }

    pendingWrites[deviceId] = true;
    invoke("set_device_effects", { deviceId, config: nextChain })
      .catch((error) => {
        handleApplyResult(
          { success: false, message: error instanceof Error ? error.message : String(error) },
          "",
        );
      })
      .finally(() => {
        delete pendingWrites[deviceId];
      });
  }

  /** Persist-only toggle for a dynamics stage (compressor/limiter/noise_gate)
   * — these are unconditionally rejected by `fx_validate::preflight` until a
   * real backing plugin exists (PD-017/#86/#18), so this never live-applies;
   * it just saves the setting to the profile for whenever one is unblocked,
   * the same persist-then-noop-until-then behavior the flat Effects panel
   * had before PD-025. */
  async function setDynamicsStageEnabled(
    deviceId: string,
    key: "compressor" | "limiter" | "noise_gate",
    enabled: boolean,
  ) {
    const chain = chains.value[deviceId] ?? emptyChain();
    const nextChain: EffectChainConfig = {
      ...chain,
      [key]: { ...chain[key], enabled },
    };
    chains.value = { ...chains.value, [deviceId]: nextChain };
    try {
      await invoke("set_device_effects", { deviceId, config: nextChain });
    } catch (error) {
      handleApplyResult(
        { success: false, message: error instanceof Error ? error.message : String(error) },
        "",
      );
    }
  }

  onMounted(async () => {
    await Promise.all([refresh(), refreshCapabilities()]);
    unlisten = await listen("graph-updated", () => {
      void refresh();
    });
  });

  onUnmounted(() => {
    unlisten?.();
  });

  return {
    chains,
    capabilities,
    loading,
    chainFor,
    refresh,
    addEq5BandStage,
    removeStage,
    reorderStages,
    scheduleStageUpdate,
    setBypassed,
    setDynamicsStageEnabled,
  };
}
