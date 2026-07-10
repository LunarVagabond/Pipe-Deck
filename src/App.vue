<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import NoticeStack from "./components/NoticeStack.vue";
import ConfirmDialog from "./components/ConfirmDialog.vue";
import Dashboard from "./views/Dashboard.vue";
import Effects from "./views/Effects.vue";
import Mixer from "./views/Mixer.vue";
import Profiles from "./views/Profiles.vue";
import Rules from "./views/Rules.vue";
import Settings from "./views/Settings.vue";
import { useApplyResult } from "./stores/notices";
import type { AppView, DaemonStatus } from "./types/graph";

const navItems = ref<{ id: AppView; label: string; enabled: boolean }[]>([
  { id: "dashboard", label: "Dashboard", enabled: true },
  { id: "profiles", label: "Profiles", enabled: true },
  { id: "rules", label: "Rules", enabled: true },
  { id: "routing", label: "Routing", enabled: false },
  { id: "mixer", label: "Mixer", enabled: true },
  { id: "sources", label: "Sources", enabled: false },
  { id: "effects", label: "Effects", enabled: true },
  { id: "settings", label: "Settings", enabled: true },
]);

const activeView = ref<AppView>("dashboard");
const daemonStatus = ref("Checking…");
const showNewModal = ref(false);
const newDeviceName = ref("");
const canCreateVirtual = computed(() => newDeviceName.value.trim().length > 0);
const { handleApplyResult } = useApplyResult();

const topbarTitle = computed(() => {
  const item = navItems.value.find((entry) => entry.id === activeView.value);
  return item?.label ?? "Overview";
});

function selectView(view: AppView, enabled: boolean) {
  if (!enabled) return;
  activeView.value = view;
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

async function createVirtual(kind: "output" | "input" | "multi") {
  const name = newDeviceName.value.trim();
  if (!name) return;
  const command =
    kind === "input"
      ? "create_virtual_input"
      : kind === "multi"
        ? "create_virtual_multi_output"
        : "create_virtual_output";
  try {
    await invoke(command, { name });
    handleApplyResult({ success: true }, `${name} created`);
    showNewModal.value = false;
    newDeviceName.value = "";
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
        <span class="brand-icon">♪</span>
        Pipe Deck
      </div>
      <a
        v-for="item in navItems"
        :key="item.id"
        class="nav-item"
        :class="{ active: item.id === activeView, disabled: !item.enabled }"
        href="#"
        @click.prevent="selectView(item.id, item.enabled)"
      >
        {{ item.label }}
      </a>
      <div class="daemon-status">
        <span class="dot" />
        {{ daemonStatus }}
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
        <Mixer v-else-if="activeView === 'mixer'" />
        <Effects v-else-if="activeView === 'effects'" />
        <Settings v-else-if="activeView === 'settings'" />
      </main>
    </div>

    <NoticeStack />
    <ConfirmDialog />

    <div v-if="showNewModal" class="new-device-modal" @click.self="showNewModal = false">
      <div class="new-device-dialog">
        <h2>Create virtual device</h2>
        <input v-model="newDeviceName" type="text" placeholder="e.g. Game Mix" />
        <p class="new-device-hint">Display name can include spaces. The system id uses dashes (Game Mix → pipe-deck-game-mix).</p>
        <div class="dialog-actions">
          <button type="button" @click="showNewModal = false">Cancel</button>
          <button type="button" class="primary" :disabled="!canCreateVirtual" @click="createVirtual('input')">Virtual input</button>
          <button type="button" class="primary" :disabled="!canCreateVirtual" @click="createVirtual('output')">Virtual output</button>
          <button type="button" class="primary" :disabled="!canCreateVirtual" @click="createVirtual('multi')">Multi output</button>
        </div>
      </div>
    </div>
  </div>
</template>
