<script setup lang="ts">
import { computed, onMounted, onUnmounted, provide, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import NoticeStack from "./components/NoticeStack.vue";
import ConfirmDialog from "./components/ConfirmDialog.vue";
import PromptDialog from "./components/PromptDialog.vue";
import ShortcutsModal from "./components/ShortcutsModal.vue";
import AppFooter from "./components/AppFooter.vue";
import NewDeviceDialog from "./components/NewDeviceDialog.vue";
import NavIcon from "./components/NavIcon.vue";
import { navigateKey } from "./composables/navigation";
import Dashboard from "./views/Dashboard.vue";
import Effects from "./views/Effects.vue";
import Mixer from "./views/Mixer.vue";
import Profiles from "./views/Profiles.vue";
import Routing from "./views/Routing.vue";
import Rules from "./views/Rules.vue";
import Settings from "./views/Settings.vue";
import Soundboard from "./views/Soundboard.vue";
import Sources from "./views/Sources.vue";
import { useApplyResult } from "./stores/notices";
import { useConfirm } from "./stores/confirm";
import { useNewDeviceDialog } from "./stores/newDeviceDialog";
import { useUpdateStatus } from "./stores/updateStatus";
import { useRuntimeGraph } from "./stores/runtimeGraph";
import { useDaemonStatus } from "./stores/daemonStatus";
import { useTheme } from "./stores/theme";
import { useShortcutsModal } from "./stores/shortcutsModal";
import type { AppConfig, AppView } from "./types/graph";

// Only views that actually wire up device creation should show the topbar's
// "+ New" action — today that's only Routing (via RoutingGraph.vue).
const NEW_DEVICE_VIEWS = new Set<AppView>(["routing"]);

const navItems = ref<
  { id: AppView; label: string; enabled: boolean; comingSoon?: boolean }[]
>([
  { id: "dashboard", label: "Dashboard", enabled: true },
  { id: "profiles", label: "Profiles", enabled: true },
  { id: "rules", label: "Rules", enabled: true },
  { id: "routing", label: "Routing", enabled: true },
  { id: "mixer", label: "Mixer", enabled: true },
  { id: "sources", label: "Sources", enabled: true },
  { id: "soundboard", label: "Soundboard", enabled: true },
  { id: "effects", label: "Effects", enabled: true },
  { id: "settings", label: "Settings", enabled: true },
]);

const activeView = ref<AppView>("dashboard");
const sidebarCollapsed = ref(false);
const { handleApplyResult } = useApplyResult();
const { openNewDeviceDialog } = useNewDeviceDialog();
const { updateStatus, updateStatusText, checkForUpdatesNow } = useUpdateStatus();
const { graph: runtimeGraph, loading: runtimeGraphLoading, error: runtimeGraphError } =
  useRuntimeGraph();
const { refreshDaemonStatus, restoreAtLoginText, restoreAtLoginClass } = useDaemonStatus();
const { resolvedKind, setMode } = useTheme();
const { openShortcutsModal } = useShortcutsModal();
const { confirm } = useConfirm();

const showNewDeviceButton = computed(() => NEW_DEVICE_VIEWS.has(activeView.value));

// Additive fast-access toggle (issue #266) — flips the explicit light/dark
// mode directly, leaving Settings' theme selector (light/dark/system) as the
// source of truth for the configured preference.
async function toggleThemeKind() {
  await setMode(resolvedKind.value === "dark" ? "light" : "dark");
}

const updateStatusDotClass = computed(() => `update-status-dot--${updateStatus.value}`);

const pipeWireStatusText = computed(() => {
  if (runtimeGraphLoading.value && !runtimeGraph.value.devices.length && !runtimeGraph.value.streams.length) {
    return "Checking…";
  }
  if (runtimeGraphError.value) return "PipeWire unreachable";
  if (runtimeGraph.value.data_source === "mock") return "Mock data";
  return "PipeWire is running";
});

const pipeWireStatusClass = computed(() => {
  if (runtimeGraphLoading.value && !runtimeGraph.value.devices.length && !runtimeGraph.value.streams.length) {
    return "status-dot--muted";
  }
  if (runtimeGraphError.value) return "status-dot--error";
  if (runtimeGraph.value.data_source === "mock") return "status-dot--muted";
  return "status-dot--ok";
});

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

async function loadPreferences() {
  try {
    const config = await invoke<AppConfig>("get_config");
    sidebarCollapsed.value = config.preferences?.sidebar_collapsed ?? false;
  } catch {
    sidebarCollapsed.value = false;
  }
}

async function toggleSidebar() {
  const next = !sidebarCollapsed.value;
  sidebarCollapsed.value = next;
  try {
    await invoke("set_sidebar_collapsed", { collapsed: next });
  } catch (error) {
    sidebarCollapsed.value = !next;
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

function isTypingTarget(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && ["INPUT", "TEXTAREA"].includes(target.tagName);
}

function onWindowKeydown(event: KeyboardEvent) {
  if (event.key !== "?" || isTypingTarget(event.target)) return;
  event.preventDefault();
  openShortcutsModal();
}

// First click of the window's X with no close-behavior preference saved yet
// (backend/tray.rs's CloseRequested handler) — the window stays open (Rust
// already prevented the close) while we ask the user to pick a default.
// set_close_behavior persists the answer AND performs the hide/exit for
// this click (apply_now: true), so the window isn't left in limbo.
async function onCloseBehaviorPromptNeeded() {
  const wantsQuit = await confirm(
    "What should happen when you click the window's close button? You can change this later in Settings.",
    {
      title: "Close button behavior",
      confirmLabel: "Quit the app",
      cancelLabel: "Minimize to background",
    },
  );
  const behavior = wantsQuit ? "quit" : "minimize";
  try {
    await invoke("set_close_behavior", { behavior, applyNow: true });
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

let unlistenCloseBehaviorPrompt: (() => void) | null = null;

onMounted(() => {
  void refreshDaemonStatus();
  void loadPreferences();
  void checkForUpdatesNow();
  window.addEventListener("keydown", onWindowKeydown);
  void listen("close-behavior-prompt-needed", () => {
    void onCloseBehaviorPromptNeeded();
  }).then((unlisten) => {
    unlistenCloseBehaviorPrompt = unlisten;
  });
});

onUnmounted(() => {
  window.removeEventListener("keydown", onWindowKeydown);
  unlistenCloseBehaviorPrompt?.();
});
</script>

<template>
  <div class="app-shell" :class="{ 'app-shell--sidebar-collapsed': sidebarCollapsed }">
    <nav class="sidebar" :class="{ 'sidebar--collapsed': sidebarCollapsed }">
      <div class="brand">
        <img class="brand-logo" src="/pipe-deck.svg" alt="" width="56" height="56" />
        <span v-show="!sidebarCollapsed" class="brand-name">Pipe Deck</span>
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
        :title="sidebarCollapsed ? item.label : item.comingSoon ? 'Coming soon' : undefined"
        href="#"
        @click.prevent="selectView(item.id, item.enabled)"
      >
        <NavIcon :kind="item.id" class="nav-item-icon" />
        <span v-show="!sidebarCollapsed" class="nav-item-label">{{ item.label }}</span>
        <span v-if="item.comingSoon" class="nav-popover" role="tooltip">Coming soon</span>
      </a>

      <div class="sidebar-footer">
        <div class="status-tray">
          <div class="status-row" :title="sidebarCollapsed ? pipeWireStatusText : undefined">
            <span class="status-dot" :class="pipeWireStatusClass" />
            <span v-show="!sidebarCollapsed" class="status-row-label">{{ pipeWireStatusText }}</span>
          </div>
          <div
            class="status-row"
            :title="sidebarCollapsed ? `Updates: ${updateStatusText}` : undefined"
          >
            <span class="status-dot" :class="updateStatusDotClass" />
            <span v-show="!sidebarCollapsed" class="status-row-label">{{ updateStatusText }}</span>
          </div>
          <div
            class="status-row"
            :title="sidebarCollapsed ? `Restore at login: ${restoreAtLoginText}` : undefined"
          >
            <span class="status-dot" :class="restoreAtLoginClass" />
            <span v-show="!sidebarCollapsed" class="status-row-label">
              Restore: {{ restoreAtLoginText }}
            </span>
          </div>
        </div>
      </div>
    </nav>

    <button
      type="button"
      class="sidebar-collapse-toggle"
      :class="{ 'sidebar-collapse-toggle--collapsed': sidebarCollapsed }"
      :aria-label="sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'"
      :title="sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'"
      @click="toggleSidebar"
    >
      <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
        <path
          d="M15 6l-6 6 6 6"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    </button>

    <div class="main-area">
      <header class="topbar">
        <div class="topbar-title">{{ topbarTitle }}</div>
        <div class="topbar-actions">
          <button
            type="button"
            class="topbar-theme-toggle"
            :aria-label="resolvedKind === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'"
            :title="resolvedKind === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'"
            @click="toggleThemeKind"
          >
            <svg v-if="resolvedKind === 'dark'" viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
              <circle cx="12" cy="12" r="4" fill="none" stroke="currentColor" stroke-width="2" />
              <path
                d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
              />
            </svg>
            <svg v-else viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
              <path
                d="M20 14.5A8.5 8.5 0 1 1 9.5 4a7 7 0 0 0 10.5 10.5z"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linejoin="round"
              />
            </svg>
          </button>
          <button
            type="button"
            class="topbar-help-btn"
            aria-label="Keyboard shortcuts"
            title="Keyboard shortcuts (?)"
            @click="openShortcutsModal()"
          >
            <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
              <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="2" />
              <path
                d="M9.5 9.5a2.5 2.5 0 1 1 3.5 2.29c-.7.32-1 .82-1 1.46V14"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
              <circle cx="12" cy="17" r="0.75" fill="currentColor" stroke="none" />
            </svg>
          </button>
          <button
            v-if="showNewDeviceButton"
            type="button"
            class="topbar-btn"
            @click="openNewDeviceDialog()"
          >
            + New
          </button>
        </div>
      </header>
      <main class="content">
        <Dashboard v-if="activeView === 'dashboard'" />
        <Profiles v-else-if="activeView === 'profiles'" />
        <Rules v-else-if="activeView === 'rules'" />
        <Routing v-else-if="activeView === 'routing'" />
        <Mixer v-else-if="activeView === 'mixer'" />
        <Sources v-else-if="activeView === 'sources'" />
        <Soundboard v-else-if="activeView === 'soundboard'" />
        <Effects v-else-if="activeView === 'effects'" />
        <Settings v-else-if="activeView === 'settings'" />
      </main>
      <AppFooter />
    </div>

    <NoticeStack />
    <ConfirmDialog />
    <PromptDialog />
    <ShortcutsModal />
    <NewDeviceDialog />
  </div>
</template>
