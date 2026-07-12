<script setup lang="ts">
import { computed, onMounted, provide, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import NoticeStack from "./components/NoticeStack.vue";
import ConfirmDialog from "./components/ConfirmDialog.vue";
import PromptDialog from "./components/PromptDialog.vue";
import AppFooter from "./components/AppFooter.vue";
import ToggleSwitch from "./components/ToggleSwitch.vue";
import { navigateKey } from "./composables/navigation";
import Dashboard from "./views/Dashboard.vue";
import Effects from "./views/Effects.vue";
import Mixer from "./views/Mixer.vue";
import Profiles from "./views/Profiles.vue";
import Routing from "./views/Routing.vue";
import Rules from "./views/Rules.vue";
import Settings from "./views/Settings.vue";
import Sources from "./views/Sources.vue";
import { useApplyResult } from "./stores/notices";
import type { AppView, DaemonStatus } from "./types/graph";

const navItems = ref<
  { id: AppView; label: string; enabled: boolean; comingSoon?: boolean }[]
>([
  { id: "dashboard", label: "Dashboard", enabled: true },
  { id: "profiles", label: "Profiles", enabled: true },
  { id: "rules", label: "Rules", enabled: true },
  { id: "routing", label: "Routing", enabled: true },
  { id: "mixer", label: "Mixer", enabled: true },
  { id: "sources", label: "Sources", enabled: true },
  { id: "effects", label: "Effects", enabled: false, comingSoon: true },
  { id: "settings", label: "Settings", enabled: true },
]);

const activeView = ref<AppView>("dashboard");
const daemonStatus = ref("Checking…");
const showNewModal = ref(false);
const newDeviceName = ref("");
const newDeviceType = ref<"input" | "output">("output");
const newDeviceMulti = ref(false);
const canCreateVirtual = computed(() => newDeviceName.value.trim().length > 0);
const { handleApplyResult } = useApplyResult();
const GITHUB_REPO = "https://github.com/LunarVagabond/Pipe-Deck";

async function openExternal(event: MouseEvent, url: string) {
  event.preventDefault();
  try {
    await invoke("open_url", { url });
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

const topbarTitle = computed(() => {
  const item = navItems.value.find((entry) => entry.id === activeView.value);
  return item?.label ?? "Overview";
});

function selectView(view: AppView, enabled: boolean) {
  if (!enabled) return;
  activeView.value = view;
}

provide(navigateKey, (view: AppView) => {
  const item = navItems.value.find((entry) => entry.id === view);
  if (item?.enabled) {
    activeView.value = view;
  }
});

function resetNewDeviceForm() {
  newDeviceName.value = "";
  newDeviceType.value = "output";
  newDeviceMulti.value = false;
}

function closeNewModal() {
  showNewModal.value = false;
  resetNewDeviceForm();
}

async function refreshDaemonStatus() {
  try {
    const status = await invoke<DaemonStatus>("get_daemon_status");
    if (status.running || status.enabled) {
      daemonStatus.value = status.running ? "Daemon active" : "Daemon enabled";
    } else {
      daemonStatus.value = "PipeWire";
    }
  } catch {
    daemonStatus.value = "PipeWire";
  }
}

onMounted(() => {
  void refreshDaemonStatus();
});

async function createVirtual() {
  const name = newDeviceName.value.trim();
  if (!name) return;
  const command =
    newDeviceType.value === "input"
      ? "create_virtual_input"
      : newDeviceMulti.value
        ? "create_virtual_multi_output"
        : "create_virtual_output";
  try {
    await invoke(command, { name });
    handleApplyResult({ success: true }, `${name} created`);
    closeNewModal();
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}
</script>

<template>
  <div class="app-shell">
    <nav class="sidebar">
      <div class="brand">
        <img class="brand-logo" src="/pipe-deck.svg" alt="" width="56" height="56" />
        <span class="brand-name">Pipe Deck</span>
      </div>
      <a
        v-for="item in navItems"
        :key="item.id"
        class="nav-item"
        :class="{
          active: item.id === activeView,
          disabled: !item.enabled,
          'has-popover': item.comingSoon,
        }"
        :aria-disabled="!item.enabled || undefined"
        :title="item.comingSoon ? 'Coming soon' : undefined"
        href="#"
        @click.prevent="selectView(item.id, item.enabled)"
      >
        {{ item.label }}
        <span v-if="item.comingSoon" class="nav-popover" role="tooltip">Coming soon</span>
      </a>
      <div class="sidebar-footer">
        <a
          class="sidebar-contribute"
          :href="GITHUB_REPO"
          @click="openExternal($event, GITHUB_REPO)"
        >
          Contribute on GitHub
        </a>
        <div class="daemon-status">
          <span class="dot" />
          {{ daemonStatus }}
        </div>
      </div>
    </nav>

    <div class="main-area">
      <header class="topbar">
        <div class="topbar-title">{{ topbarTitle }}</div>
        <div class="topbar-actions">
          <button type="button" class="topbar-btn" @click="showNewModal = true">+ New</button>
        </div>
      </header>
      <main class="content">
        <Dashboard v-if="activeView === 'dashboard'" />
        <Profiles v-else-if="activeView === 'profiles'" />
        <Rules v-else-if="activeView === 'rules'" />
        <Routing v-else-if="activeView === 'routing'" />
        <Mixer v-else-if="activeView === 'mixer'" />
        <Sources v-else-if="activeView === 'sources'" />
        <Effects v-else-if="activeView === 'effects'" />
        <Settings v-else-if="activeView === 'settings'" />
      </main>
      <AppFooter />
    </div>

    <NoticeStack />
    <ConfirmDialog />
    <PromptDialog />

    <div v-if="showNewModal" class="new-device-modal" @click.self="closeNewModal">
      <div class="new-device-dialog">
        <h2>Create Virtual Device</h2>
        <div class="new-device-field">
          <label class="new-device-field-label" for="new-device-name">Name</label>
          <input
            id="new-device-name"
            v-model="newDeviceName"
            type="text"
            placeholder="e.g. Game Mix"
          />
        </div>
        <div class="new-device-field">
          <label class="new-device-field-label" for="new-device-type">Type</label>
          <select id="new-device-type" v-model="newDeviceType">
            <option value="input">Input</option>
            <option value="output">Output</option>
          </select>
        </div>
        <div v-if="newDeviceType === 'output'" class="new-device-toggle-row">
          <span class="new-device-field-label">Multi-output</span>
          <ToggleSwitch v-model="newDeviceMulti" :show-state-labels="false" />
        </div>
        <p class="new-device-hint">
          Display name can include spaces. The system id uses dashes (Game Mix → pipe-deck-game-mix).
        </p>
        <div class="dialog-actions">
          <button type="button" @click="closeNewModal">Cancel</button>
          <button
            type="button"
            class="primary"
            :disabled="!canCreateVirtual"
            @click="createVirtual"
          >
            Create
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
