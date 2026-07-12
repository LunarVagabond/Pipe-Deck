<script setup lang="ts">
import { computed, onMounted, provide, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import NoticeStack from "./components/NoticeStack.vue";
import ConfirmDialog from "./components/ConfirmDialog.vue";
import PromptDialog from "./components/PromptDialog.vue";
import AppFooter from "./components/AppFooter.vue";
import NewDeviceDialog from "./components/NewDeviceDialog.vue";
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
    <NewDeviceDialog v-model="showNewModal" />
  </div>
</template>
