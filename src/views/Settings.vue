<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import ToggleSwitch from "../components/ToggleSwitch.vue";
import SegmentedControl from "../components/SegmentedControl.vue";
import { useApplyResult } from "../stores/notices";
import { useUpdateStatus } from "../stores/updateStatus";
import { useTheme } from "../stores/theme";
import { useDaemonStatus } from "../stores/daemonStatus";
import type { PluginStatus } from "../types/graph";
import type { ThemeMode } from "../types/theme";

const THEME_MODE_OPTIONS = [
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "system", label: "Follow system" },
];

type SettingsTab = "general" | "background" | "plugins" | "about";

const tabs: { id: SettingsTab; label: string }[] = [
  { id: "general", label: "General" },
  { id: "background", label: "Background" },
  { id: "plugins", label: "Plugins" },
  { id: "about", label: "About" },
];

const activeTab = ref<SettingsTab>("general");
const restoreOnStartup = ref(true);
const backgroundRestore = ref(false);
const autoApplyRules = ref(true);
const { daemonStatus, refreshDaemonStatus, lastRunText } = useDaemonStatus();
const plugins = ref<PluginStatus[]>([]);
const busy = ref(false);
const { handleApplyResult } = useApplyResult();
const configPaths = ref<{ configDir: string; profilesDir: string } | null>(null);
const {
  appInfo,
  updateResult,
  checkingUpdates,
  updateStatus,
  updateStatusText,
  ensureAppInfo,
  checkForUpdatesNow,
  installUpdateNow,
} = useUpdateStatus();
const {
  schemes,
  mode: themeMode,
  darkSchemeId,
  lightSchemeId,
  resolvedKind,
  setMode: setThemeMode,
  setDarkScheme,
  setLightScheme,
} = useTheme();

const darkSchemes = computed(() => schemes.value.filter((scheme) => scheme.kind === "dark"));
const lightSchemes = computed(() => schemes.value.filter((scheme) => scheme.kind === "light"));

const BMC_URL = "https://www.buymeacoffee.com/lunarvagabond";
const BMC_BUTTON_SRC = "https://cdn.buymeacoffee.com/buttons/v2/default-violet.png";
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

const updateStatusClass = computed(() => `update-status-dot--${updateStatus.value}`);

const backgroundRestoreHint = computed(() => {
  if (appInfo.value?.backgroundRestoreSupported) {
    return "Installs a user systemd unit for restore at login.";
  }
  return `Not supported in the Flatpak build. Install via ${nativeInstallHint.value} for login restore.`;
});

const nativeInstallHint = computed(() => {
  const kind = appInfo.value?.installKind;
  if (kind === "deb") return ".deb";
  if (kind === "rpm") return ".rpm";
  if (kind === "app_image") return "AppImage";
  return ".deb, .rpm, or AppImage";
});

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
  await refreshDaemonStatus();
  plugins.value = await invoke("list_plugins");
  await ensureAppInfo();
  configPaths.value = await invoke("get_config_paths");
}

async function copyPath(path: string) {
  try {
    await navigator.clipboard.writeText(path);
    handleApplyResult({ success: true }, "Path copied to clipboard.");
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
}

async function runUpdateCheck() {
  await checkForUpdatesNow();
}

async function applyUpdate() {
  try {
    await installUpdateNow();
    handleApplyResult({ success: true }, "Update started");
  } catch (error) {
    handleApplyResult(
      { success: false, message: error instanceof Error ? error.message : String(error) },
      "",
    );
  }
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

      <p class="settings-subheading">Appearance</p>

      <div class="settings-row">
        <div>
          <p class="settings-row-label">Mode</p>
          <p class="settings-row-hint">
            Choose Light or Dark, or follow your system's theme automatically.
          </p>
        </div>
        <SegmentedControl
          :model-value="themeMode"
          :options="THEME_MODE_OPTIONS"
          @update:model-value="(value) => setThemeMode(value as ThemeMode)"
        />
      </div>

      <div class="settings-row">
        <div>
          <p class="settings-row-label">Dark scheme</p>
          <p class="settings-row-hint">Used when the app is in Dark mode.</p>
        </div>
        <div class="scheme-select-group">
          <span v-if="resolvedKind === 'dark'" class="scheme-active-badge">Active</span>
          <select
            class="scheme-select"
            :value="darkSchemeId"
            @change="setDarkScheme(($event.target as HTMLSelectElement).value)"
          >
            <option v-for="scheme in darkSchemes" :key="scheme.id" :value="scheme.id">
              {{ scheme.name }}
            </option>
          </select>
        </div>
      </div>

      <div class="settings-row">
        <div>
          <p class="settings-row-label">Light scheme</p>
          <p class="settings-row-hint">Used when the app is in Light mode.</p>
        </div>
        <div class="scheme-select-group">
          <span v-if="resolvedKind === 'light'" class="scheme-active-badge">Active</span>
          <select
            class="scheme-select"
            :value="lightSchemeId"
            @change="setLightScheme(($event.target as HTMLSelectElement).value)"
          >
            <option v-for="scheme in lightSchemes" :key="scheme.id" :value="scheme.id">
              {{ scheme.name }}
            </option>
          </select>
        </div>
      </div>

      <p class="settings-row-hint">
        Want to design your own? Drop a YAML file in
        <code>~/.config/pipe-deck/themes/</code> — see the
        <a href="https://github.com/LunarVagabond/Pipe-Deck/wiki/Theming" target="_blank" rel="noreferrer">Theming docs</a>
        for the schema.
      </p>
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

      <div v-if="appInfo?.backgroundRestoreSupported" class="settings-row">
        <div>
          <p class="settings-row-label">Restore at login via background service</p>
          <p class="settings-row-hint">
            {{ backgroundRestoreHint }}
          </p>
        </div>
        <ToggleSwitch
          :model-value="backgroundRestore"
          :disabled="busy"
          @update:model-value="setBackgroundRestore"
        />
      </div>

      <div v-else class="settings-row settings-row--static">
        <div>
          <p class="settings-row-label">Restore at login via background service</p>
          <p class="settings-row-hint">
            {{ backgroundRestoreHint }}
          </p>
        </div>
        <span class="settings-unsupported-pill">Not supported</span>
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
            <dd>{{ lastRunText }}</dd>
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

    <div
      v-show="activeTab === 'about'"
      class="settings-panel"
      role="tabpanel"
      aria-labelledby="settings-tab-about"
    >
      <p class="settings-panel-lead">
        Version info and update checks. Pipe Deck will eventually check once at startup unless
        dismissed.
      </p>

      <div class="settings-row settings-row--static">
        <div>
          <p class="settings-row-label">Installed version</p>
          <p class="settings-row-hint">
            {{ appInfo?.buildRevision ?? "…" }}
            <template v-if="appInfo?.installLabel"> · {{ appInfo.installLabel }}</template>
          </p>
        </div>
      </div>

      <div class="settings-row settings-row--static">
        <div>
          <p class="settings-row-label">Config directory</p>
          <p class="settings-row-hint">{{ configPaths?.configDir ?? "…" }}</p>
        </div>
        <button
          type="button"
          class="settings-action-btn"
          :disabled="!configPaths"
          @click="copyPath(configPaths!.configDir)"
        >
          Copy
        </button>
      </div>

      <div class="settings-row settings-row--static">
        <div>
          <p class="settings-row-label">Profiles directory</p>
          <p class="settings-row-hint">{{ configPaths?.profilesDir ?? "…" }}</p>
        </div>
        <button
          type="button"
          class="settings-action-btn"
          :disabled="!configPaths"
          @click="copyPath(configPaths!.profilesDir)"
        >
          Copy
        </button>
      </div>

      <div class="settings-row settings-row--static">
        <div>
          <p class="settings-row-label">Contribute</p>
          <p class="settings-row-hint">Pipe Deck is open source — issues and PRs are welcome.</p>
        </div>
        <a
          class="settings-action-btn"
          :href="GITHUB_REPO"
          @click="openExternal($event, GITHUB_REPO)"
        >
          Contribute on GitHub
        </a>
      </div>

      <div class="settings-row settings-support-row">
        <div>
          <p class="settings-row-label">Support Pipe Deck</p>
          <p class="settings-row-hint">Enjoying the app? Consider chipping in.</p>
        </div>
        <a
          class="settings-bmc"
          :href="BMC_URL"
          aria-label="Buy me a coffee"
          @click="openExternal($event, BMC_URL)"
        >
          <img :src="BMC_BUTTON_SRC" alt="Buy me a coffee" width="162" height="45" />
        </a>
      </div>

      <div class="settings-row">
        <div class="settings-update-copy">
          <p class="settings-row-label settings-update-label">
            <span class="update-status-dot" :class="updateStatusClass" aria-hidden="true" />
            Check for updates
          </p>
          <p class="settings-row-hint">
            <template v-if="updateStatus === 'checking'">Checking GitHub releases…</template>
            <template v-else-if="updateResult?.latestVersion">
              {{ updateStatusText }} —
              latest is v{{ updateResult.latestVersion }}
            </template>
            <template v-else>
              {{ updateResult?.error ?? "Run a check to compare with the latest release." }}
            </template>
          </p>
        </div>
        <div class="settings-update-actions">
          <button
            type="button"
            class="settings-action-btn"
            :disabled="checkingUpdates || !appInfo"
            @click="runUpdateCheck"
          >
            {{ checkingUpdates ? "Checking…" : "Check now" }}
          </button>
          <button
            v-if="
              updateResult &&
              (updateStatus === 'outdated' || updateStatus === 'severely_outdated')
            "
            type="button"
            class="settings-action-btn settings-action-btn--primary"
            @click="applyUpdate"
          >
            {{ updateResult.canAutoInstall ? "Install update" : "Get update" }}
          </button>
        </div>
      </div>
    </div>
  </section>
</template>
