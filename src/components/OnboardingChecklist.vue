<script setup lang="ts">
import { computed, inject, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { navigateKey } from "../composables/navigation";
import { useAppConfig, useRuntimeGraph } from "../stores/runtimeGraph";
import { useApplyResult } from "../stores/notices";

// Issue #5: a lightweight, non-blocking first-run checklist — a dismissible
// panel (never a modal/wizard), each item's done-state read live from
// already-fetched app state rather than a static flag, so completing a step
// outside the checklist (e.g. setting the default output in a system tray
// applet) is reflected here too.
const PIPEWIRE_DOCS_URL =
  "https://github.com/LunarVagabond/Pipe-Deck/blob/main/docs/developers/Getting_Started.md#prerequisites";

const { graph, loading, error } = useRuntimeGraph();
const { config, profiles } = useAppConfig();
const { handleApplyResult } = useApplyResult();
const navigate = inject(navigateKey);

const dismissed = ref(true);

// Default to hidden until we actually know the persisted state, so the
// panel never flashes on then off while `get_config` resolves.
watch(
  config,
  (value) => {
    if (value) {
      dismissed.value = value.preferences?.onboarding_dismissed ?? false;
    }
  },
  { immediate: true },
);

const pipewireDetected = computed(() => !error.value && graph.value.devices.length > 0);
const defaultOutputSet = computed(() => Boolean(graph.value.default_output_system_name));
const profileCreated = computed(() => profiles.value.length > 0);

async function openPipewireDocs() {
  try {
    await invoke("open_url", { url: PIPEWIRE_DOCS_URL });
  } catch (openError) {
    handleApplyResult(
      { success: false, message: openError instanceof Error ? openError.message : String(openError) },
      "",
    );
  }
}

function openSources() {
  navigate?.("sources");
}

function openProfiles() {
  navigate?.("profiles");
}

interface ChecklistItem {
  id: string;
  label: string;
  done: boolean;
  actionLabel: string;
  action: () => void;
}

const items = computed<ChecklistItem[]>(() => [
  {
    id: "pipewire-detected",
    label: "PipeWire detected",
    done: pipewireDetected.value,
    actionLabel: "View setup docs",
    action: openPipewireDocs,
  },
  {
    id: "default-output-set",
    label: "Default output set",
    done: defaultOutputSet.value,
    actionLabel: "Open Sources",
    action: openSources,
  },
  {
    id: "profile-created",
    label: "Profile created (optional)",
    done: profileCreated.value,
    actionLabel: "Open Profiles",
    action: openProfiles,
  },
]);

const visible = computed(() => Boolean(config.value) && !dismissed.value && !loading.value);

async function dismiss() {
  const previous = dismissed.value;
  dismissed.value = true;
  try {
    await invoke("set_onboarding_dismissed", { dismissed: true });
    if (config.value) {
      config.value = {
        ...config.value,
        preferences: {
          ...config.value.preferences,
          onboarding_dismissed: true,
        },
      };
    }
  } catch (dismissError) {
    dismissed.value = previous;
    handleApplyResult(
      {
        success: false,
        message: dismissError instanceof Error ? dismissError.message : String(dismissError),
      },
      "",
    );
  }
}
</script>

<template>
  <div v-if="visible" class="onboarding-checklist" role="complementary" aria-label="Getting started checklist">
    <div class="onboarding-checklist-header">
      <h2>Getting started</h2>
      <button
        type="button"
        class="onboarding-checklist-close"
        title="Dismiss — this won't show again"
        aria-label="Dismiss checklist"
        @click="dismiss"
      >
        <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
          <path
            d="M6 6l12 12M18 6L6 18"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
          />
        </svg>
      </button>
    </div>
    <ul class="onboarding-checklist-list">
      <li
        v-for="item in items"
        :key="item.id"
        class="onboarding-checklist-item"
        :class="{ 'onboarding-checklist-item--done': item.done }"
      >
        <span class="onboarding-checklist-item-check" aria-hidden="true">
          <svg v-if="item.done" viewBox="0 0 24 24" width="12" height="12">
            <path
              d="M4 12l5 5L20 6"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </span>
        <div class="onboarding-checklist-item-body">
          <span class="onboarding-checklist-item-label">{{ item.label }}</span>
          <button
            v-if="!item.done"
            type="button"
            class="link-btn onboarding-checklist-item-action"
            @click="item.action"
          >
            {{ item.actionLabel }}
          </button>
        </div>
      </li>
    </ul>
  </div>
</template>
