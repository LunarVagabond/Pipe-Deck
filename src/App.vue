<script setup lang="ts">
import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import NoticeStack from "./components/NoticeStack.vue";
import Dashboard from "./views/Dashboard.vue";
import Profiles from "./views/Profiles.vue";
import { useApplyResult } from "./stores/notices";
import type { AppView } from "./types/graph";

const navItems: { id: AppView; label: string; enabled: boolean }[] = [
  { id: "dashboard", label: "Dashboard", enabled: true },
  { id: "profiles", label: "Profiles", enabled: true },
  { id: "routing", label: "Routing", enabled: false },
  { id: "mixer", label: "Mixer", enabled: false },
  { id: "sources", label: "Sources", enabled: false },
  { id: "effects", label: "Effects", enabled: false },
  { id: "settings", label: "Settings", enabled: false },
];

const activeView = ref<AppView>("dashboard");
const daemonStatus = ref("PipeWire");
const showNewModal = ref(false);
const newDeviceName = ref("");
const { handleApplyResult } = useApplyResult();

const topbarTitle = computed(() => {
  const item = navItems.find((entry) => entry.id === activeView.value);
  return item?.label ?? "Overview";
});

function selectView(view: AppView, enabled: boolean) {
  if (!enabled) return;
  activeView.value = view;
}

async function createVirtual(kind: "output" | "input") {
  const name = newDeviceName.value.trim() || (kind === "output" ? "Virtual Output" : "Virtual Input");
  const command = kind === "output" ? "create_virtual_output" : "create_virtual_input";
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
      </main>
    </div>

    <NoticeStack />

    <div v-if="showNewModal" class="new-device-modal" @click.self="showNewModal = false">
      <div class="new-device-dialog">
        <h2>Create virtual device</h2>
        <input v-model="newDeviceName" type="text" placeholder="Device name" />
        <div class="dialog-actions">
          <button type="button" @click="showNewModal = false">Cancel</button>
          <button type="button" class="primary" @click="createVirtual('input')">Virtual input</button>
          <button type="button" class="primary" @click="createVirtual('output')">Virtual output</button>
        </div>
      </div>
    </div>
  </div>
</template>
