<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import ToggleSwitch from "../components/ToggleSwitch.vue";
import { useApplyResult } from "../stores/notices";
import type { DaemonStatus, PluginStatus } from "../types/graph";

type SettingsTab = "general" | "background" | "plugins";

const tabs: { id: SettingsTab; label: string }[] = [
  { id: "general", label: "General" },
  { id: "background", label: "Background" },
  { id: "plugins", label: "Plugins" },
];

const activeTab = ref<SettingsTab>("general");
const restoreOnStartup = ref(true);
const backgroundRestore = ref(false);
const autoApplyRules = ref(true);
const daemonStatus = ref<DaemonStatus | null>(null);
const plugins = ref<PluginStatus[]>([]);
const busy = ref(false);
const { handleApplyResult } = useApplyResult();

async function loadSettings() {
  const config = await invoke<{
    preferences?: {
      restore_on_startup?: boolean;
      background_restore?: boolean;
      auto_apply_rules?: boolean;
    };
  }>("get_config");
  restoreOnStartup.value = config.preferences?.restore_on_startup ?? true;
  backgroundRestore.value = config.preferences?.background_restore ?? false;
  autoApplyRules.value = config.preferences?.auto_apply_rules ?? true;
  daemonStatus.value = await invoke("get_daemon_status");
  plugins.value = await invoke("list_plugins");
}

async function setRestoreOnStartup(enabled: boolean) {
  restoreOnStartup.value = enabled;
  busy.value = true;
  try {
    await invoke("set_restore_on_startup", { enabled });
    handleApplyResult({ success: true }, "Startup restore preference saved");
  } catch (error) {
    restoreOnStartup.value = !enabled;
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  } finally {
    busy.value = false;
  }
}

async function setAutoApplyRules(enabled: boolean) {
  autoApplyRules.value = enabled;
  busy.value = true;
  try {
    await invoke("set_auto_apply_rules", { enabled });
    handleApplyResult({ success: true }, "Auto-apply rules preference saved");
  } catch (error) {
    autoApplyRules.value = !enabled;
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  } finally {
    busy.value = false;
  }
}

async function setBackgroundRestore(enabled: boolean) {
  backgroundRestore.value = enabled;
  busy.value = true;
  try {
    if (enabled) {
      await invoke("enable_background_restore");
      handleApplyResult({ success: true }, "Background restore enabled");
    } else {
      await invoke("disable_background_restore");
      handleApplyResult({ success: true }, "Background restore disabled");
    }
    await loadSettings();
  } catch (error) {
    backgroundRestore.value = !enabled;
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  } finally {
    busy.value = false;
  }
}

async function togglePlugin(plugin: PluginStatus, enabled: boolean) {
  busy.value = true;
  try {
    await invoke("set_plugin_enabled", { pluginId: plugin.id, enabled });
    await loadSettings();
    handleApplyResult({ success: true }, `${plugin.name} ${enabled ? "enabled" : "disabled"}`);
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  } finally {
    busy.value = false;
  }
}

async function toggleCapability(plugin: PluginStatus, capability: string, granted: boolean) {
  const next = new Set(plugin.granted_capabilities);
  if (granted) {
    next.add(capability);
  } else {
    next.delete(capability);
  }
  busy.value = true;
  try {
    await invoke("grant_plugin_capabilities", {
      pluginId: plugin.id,
      capabilities: Array.from(next),
    });
    await loadSettings();
    handleApplyResult({ success: true }, "Plugin capabilities updated");
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  } finally {
    busy.value = false;
  }
}

onMounted(() => {
  void loadSettings();
});
</script>

<template>
  <section class="settings-view">
    <header class="settings-header view-header">
      <div>
        <p class="eyebrow">Preferences</p>
        <h1>Settings</h1>
        <p class="settings-lead">
          App behavior, background restore, and plugin permissions.
        </p>
      </div>
    </header>

    <div class="settings-tabs" role="tablist" aria-label="Settings sections">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        type="button"
        role="tab"
        class="settings-tab"
        :class="{ active: activeTab === tab.id }"
        :aria-selected="activeTab === tab.id"
        @click="activeTab = tab.id"
      >
        {{ tab.label }}
      </button>
    </div>

    <div
      v-show="activeTab === 'general'"
      class="settings-panel"
      role="tabpanel"
      aria-labelledby="settings-tab-general"
    >
      <p class="settings-panel-lead">
        Control how Pipe Deck restores your routing and applies rules when apps start.
      </p>

      <div class="settings-row">
        <div>
          <p class="settings-row-label">Restore virtual devices when the app opens</p>
          <p class="settings-row-hint">
            Re-creates saved virtual devices and routing from your active profile on launch.
          </p>
        </div>
        <ToggleSwitch
          :model-value="restoreOnStartup"
          :disabled="busy"
          @update:model-value="setRestoreOnStartup"
        />
      </div>

      <div class="settings-row">
        <div>
          <p class="settings-row-label">Auto-apply rules when new apps appear</p>
          <p class="settings-row-hint">
            Matching rules route new streams automatically. Manual changes override rules until you
            click Apply rules.
          </p>
        </div>
        <ToggleSwitch
          :model-value="autoApplyRules"
          :disabled="busy"
          @update:model-value="setAutoApplyRules"
        />
      </div>
    </div>

    <div
      v-show="activeTab === 'background'"
      class="settings-panel"
      role="tabpanel"
      aria-labelledby="settings-tab-background"
    >
      <p class="settings-panel-lead">
        Run restore at login via a user systemd service, even when the app is closed.
      </p>

      <div class="settings-row">
        <div>
          <p class="settings-row-label">Restore at login via background service</p>
          <p class="settings-row-hint">
            Installs a user systemd unit. Flatpak installs may not support user systemd units.
          </p>
        </div>
        <ToggleSwitch
          :model-value="backgroundRestore"
          :disabled="busy"
          @update:model-value="setBackgroundRestore"
        />
      </div>

      <div class="settings-status-section">
        <p class="settings-status-heading">Service status</p>
        <dl class="settings-status-grid">
          <div>
            <dt>Service enabled</dt>
            <dd>{{ daemonStatus?.enabled ? "Yes" : "No" }}</dd>
          </div>
          <div>
            <dt>Last run active</dt>
            <dd>{{ daemonStatus?.running ? "Yes" : "No" }}</dd>
          </div>
          <div>
            <dt>Last run</dt>
            <dd>{{ daemonStatus?.last_run ?? "Never" }}</dd>
          </div>
          <div>
            <dt>Devices restored</dt>
            <dd>{{ daemonStatus?.devices_restored ?? 0 }}</dd>
          </div>
        </dl>
        <p v-if="daemonStatus?.last_error" class="settings-error">
          {{ daemonStatus.last_error }}
        </p>
      </div>
    </div>

    <div
      v-show="activeTab === 'plugins'"
      class="settings-panel"
      role="tabpanel"
      aria-labelledby="settings-tab-plugins"
    >
      <p class="settings-panel-lead">
        Enable extensions and grant the capabilities each plugin requests.
      </p>

      <p v-if="plugins.length === 0" class="settings-hint">No plugins discovered.</p>

      <div v-for="plugin in plugins" :key="plugin.id" class="plugin-card">
        <div class="settings-row">
          <div>
            <strong>{{ plugin.name }}</strong>
            <span class="plugin-meta">v{{ plugin.version }} · {{ plugin.runtime_status }}</span>
            <p v-if="plugin.description" class="settings-row-hint">{{ plugin.description }}</p>
          </div>
          <ToggleSwitch
            :model-value="plugin.enabled"
            :disabled="busy"
            @update:model-value="(enabled) => togglePlugin(plugin, enabled)"
          />
        </div>
        <div v-if="plugin.requested_capabilities.length > 0" class="plugin-capabilities">
          <p class="plugin-capabilities-label">Capabilities</p>
          <div
            v-for="capability in plugin.requested_capabilities"
            :key="capability"
            class="settings-row plugin-capability-row"
          >
            <p class="settings-row-label">{{ capability }}</p>
            <ToggleSwitch
              :model-value="plugin.granted_capabilities.includes(capability)"
              :disabled="busy || !plugin.enabled"
              :show-state-labels="false"
              @update:model-value="(granted) => toggleCapability(plugin, capability, granted)"
            />
          </div>
        </div>
        <p v-if="plugin.last_error" class="settings-error">{{ plugin.last_error }}</p>
      </div>

      <p class="settings-footnote">Audit log: ~/.local/state/pipe-deck/plugin-audit.jsonl</p>
    </div>
  </section>
</template>
