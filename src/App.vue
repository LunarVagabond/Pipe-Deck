<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import NoticeStack from "./components/NoticeStack.vue";
import ConfirmDialog from "./components/ConfirmDialog.vue";
import Dashboard from "./views/Dashboard.vue";
import Profiles from "./views/Profiles.vue";
import Rules from "./views/Rules.vue";
import Settings from "./views/Settings.vue";
import { useApplyResult } from "./stores/notices";
import type { AppView, DaemonStatus, PluginUiPanel } from "./types/graph";

const navItems = ref<{ id: AppView; label: string; enabled: boolean }[]>([
  { id: "dashboard", label: "Dashboard", enabled: true },
  { id: "profiles", label: "Profiles", enabled: true },
  { id: "rules", label: "Rules", enabled: true },
  { id: "routing", label: "Routing", enabled: false },
  { id: "mixer", label: "Mixer", enabled: false },
  { id: "sources", label: "Sources", enabled: false },
  { id: "effects", label: "Effects", enabled: false },
  { id: "settings", label: "Settings", enabled: true },
]);

const effectPanels = ref<PluginUiPanel[]>([]);

const activeView = ref<AppView>("dashboard");
const daemonStatus = ref("Checking…");
const showNewModal = ref(false);
const newDeviceName = ref("");
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
  void refreshPluginPanels();
});

async function refreshPluginPanels() {
  try {
    effectPanels.value = await invoke<PluginUiPanel[]>("list_plugin_ui_panels");
    const effects = navItems.value.find((item) => item.id === "effects");
    if (effects) {
      effects.enabled = effectPanels.value.length > 0;
    }
  } catch {
    effectPanels.value = [];
  }
}

async function createVirtual(kind: "output" | "input" | "multi") {
  const name = newDeviceName.value.trim() || (kind === "input" ? "Virtual Input" : "Virtual Output");
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
        <section v-else-if="activeView === 'effects'" class="effects-view">
          <h1>Effects</h1>
          <article v-for="panel in effectPanels" :key="panel.id" class="effects-panel">
            <h2>{{ panel.title }}</h2>
            <p>{{ panel.summary }}</p>
          </article>
        </section>
        <Settings v-else-if="activeView === 'settings'" />
      </main>
    </div>

    <NoticeStack />
    <ConfirmDialog />

    <div v-if="showNewModal" class="new-device-modal" @click.self="showNewModal = false">
      <div class="new-device-dialog">
        <h2>Create virtual device</h2>
        <input v-model="newDeviceName" type="text" placeholder="Device name" />
        <div class="dialog-actions">
          <button type="button" @click="showNewModal = false">Cancel</button>
          <button type="button" class="primary" @click="createVirtual('input')">Virtual input</button>
          <button type="button" class="primary" @click="createVirtual('output')">Virtual output</button>
          <button type="button" class="primary" @click="createVirtual('multi')">Multi output</button>
        </div>
      </div>
    </div>
  </div>
</template>
