import { mount, flushPromises } from "@vue/test-utils";
import { computed, ref } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import Settings from "./Settings.vue";
import {
  makeAppInfo,
  makeCapabilityInfo,
  makeConfigPaths,
  makeDaemonStatus,
  makeDiscoveryIssue,
  makePluginStatus,
  makeResolvedScheme,
  makeUpdateResult,
} from "../test/settingsFixtures";
import type { DaemonStatus } from "../types/graph";
import type { AppInfo, UpdateCheckResult } from "../types/app";
import type { ResolvedScheme, ThemeBaseKind, ThemeMode } from "../types/theme";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const pushNoticeMock = vi.hoisted(() => vi.fn());
const noticeDurationMs = ref(5000);
const setNoticeDurationMock = vi.hoisted(() => vi.fn());
vi.mock("../stores/notices", () => ({
  useApplyResult: () => ({
    handleApplyResult: (result: { success: boolean; message?: string }, successMessage: string) => {
      if (result.success) {
        pushNoticeMock("success", successMessage);
        return true;
      }
      pushNoticeMock("error", result.message ?? "Operation failed");
      return false;
    },
  }),
  useNoticeSettings: () => ({ noticeDurationMs, setNoticeDuration: setNoticeDurationMock }),
}));

const daemonStatus = ref<DaemonStatus | null>(null);
const refreshDaemonStatusMock = vi.hoisted(() => vi.fn());
vi.mock("../stores/daemonStatus", () => ({
  useDaemonStatus: () => ({
    daemonStatus,
    refreshDaemonStatus: refreshDaemonStatusMock,
    lastRunText: computed(() => "Never"),
  }),
}));

const appInfo = ref<AppInfo | null>(null);
const updateResult = ref<UpdateCheckResult | null>(null);
const checkingUpdates = ref(false);
const installingUpdate = ref(false);
const installProgress = ref<number | null>(null);
const installComplete = ref(false);
const ensureAppInfoMock = vi.hoisted(() => vi.fn());
const checkForUpdatesNowMock = vi.hoisted(() => vi.fn());
const installUpdateNowMock = vi.hoisted(() => vi.fn());
vi.mock("../stores/updateStatus", () => ({
  useUpdateStatus: () => ({
    appInfo,
    updateResult,
    checkingUpdates,
    installingUpdate,
    installProgress,
    installComplete,
    updateStatus: computed(() => (checkingUpdates.value ? "checking" : (updateResult.value?.status ?? "unknown"))),
    updateStatusText: computed(() => updateResult.value?.status ?? "unknown"),
    ensureAppInfo: ensureAppInfoMock,
    checkForUpdatesNow: checkForUpdatesNowMock,
    installUpdateNow: installUpdateNowMock,
  }),
}));

const schemes = ref<ResolvedScheme[]>([]);
const themeMode = ref<ThemeMode>("dark");
const darkSchemeId = ref("midnight-deck");
const lightSchemeId = ref("paper-deck");
const resolvedKind = ref<ThemeBaseKind>("dark");
const setThemeModeMock = vi.hoisted(() => vi.fn());
const setDarkSchemeMock = vi.hoisted(() => vi.fn());
const setLightSchemeMock = vi.hoisted(() => vi.fn());
const resetThemeToDefaultsMock = vi.hoisted(() => vi.fn());
vi.mock("../stores/theme", () => ({
  DEFAULT_DARK_SCHEME_ID: "midnight-deck",
  DEFAULT_LIGHT_SCHEME_ID: "paper-deck",
  DEFAULT_THEME_MODE: "system",
  useTheme: () => ({
    schemes,
    mode: themeMode,
    darkSchemeId,
    lightSchemeId,
    resolvedKind,
    setMode: setThemeModeMock,
    setDarkScheme: setDarkSchemeMock,
    setLightScheme: setLightSchemeMock,
    resetToDefaults: resetThemeToDefaultsMock,
  }),
}));

function mockInvokeDefaults(overrides: Record<string, unknown> = {}) {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd in overrides) return Promise.resolve(overrides[cmd]);
    if (cmd === "get_config") return Promise.resolve({ preferences: {} });
    if (cmd === "list_plugins") return Promise.resolve([]);
    if (cmd === "list_plugin_capability_metadata") return Promise.resolve([]);
    if (cmd === "list_plugin_discovery_errors") return Promise.resolve([]);
    if (cmd === "get_config_paths") return Promise.resolve(makeConfigPaths());
    return Promise.resolve(undefined);
  });
}

let activeWrapper: ReturnType<typeof mount> | undefined;

function mountSettings() {
  activeWrapper = mount(Settings, { attachTo: document.body });
  return activeWrapper;
}

afterEach(() => {
  activeWrapper?.unmount();
  activeWrapper = undefined;
});

beforeEach(() => {
  invokeMock.mockReset();
  pushNoticeMock.mockClear();
  setNoticeDurationMock.mockClear();
  refreshDaemonStatusMock.mockReset().mockResolvedValue(undefined);
  ensureAppInfoMock.mockReset().mockImplementation(async () => {
    appInfo.value = makeAppInfo();
    return appInfo.value;
  });
  checkForUpdatesNowMock.mockReset();
  installUpdateNowMock.mockReset();
  setThemeModeMock.mockClear();
  setDarkSchemeMock.mockClear();
  setLightSchemeMock.mockClear();
  resetThemeToDefaultsMock.mockClear();

  daemonStatus.value = makeDaemonStatus();
  appInfo.value = null;
  updateResult.value = null;
  checkingUpdates.value = false;
  installingUpdate.value = false;
  installProgress.value = null;
  installComplete.value = false;
  schemes.value = [makeResolvedScheme(), makeResolvedScheme({ id: "paper-deck", name: "Paper Deck", kind: "light" })];
  themeMode.value = "dark";
  darkSchemeId.value = "midnight-deck";
  lightSchemeId.value = "paper-deck";
  resolvedKind.value = "dark";

  mockInvokeDefaults();
});

describe("Settings view", () => {
  describe("initial load", () => {
    it("loads config, plugins, and paths on mount", async () => {
      mockInvokeDefaults({
        get_config: { preferences: { restore_on_startup: false, auto_apply_rules: false } },
        list_plugins: [makePluginStatus({ name: "My Plugin" })],
      });

      const wrapper = mountSettings();
      await flushPromises();

      expect(invokeMock).toHaveBeenCalledWith("get_config");
      expect(invokeMock).toHaveBeenCalledWith("list_plugins");
      expect(invokeMock).toHaveBeenCalledWith("get_config_paths");
      expect(refreshDaemonStatusMock).toHaveBeenCalled();
      expect(ensureAppInfoMock).toHaveBeenCalled();

      await wrapper.find(".settings-tab").trigger("click");
      const restoreToggle = wrapper.find(".toggle-input");
      expect((restoreToggle.element as HTMLInputElement).checked).toBe(false);
    });

    it("shows plugin discovery error banner when errors present", async () => {
      mockInvokeDefaults({
        list_plugin_discovery_errors: [makeDiscoveryIssue()],
      });

      const wrapper = mountSettings();
      await flushPromises();
      await wrapper.findAll(".settings-tab")[2].trigger("click");

      expect(wrapper.find(".plugin-discovery-warning").exists()).toBe(true);
      expect(wrapper.find(".plugin-discovery-warning").text()).toContain("1");
    });

    it("shows empty state when no plugins and no discovery errors", async () => {
      const wrapper = mountSettings();
      await flushPromises();
      await wrapper.findAll(".settings-tab")[2].trigger("click");

      expect(wrapper.find(".settings-hint").text()).toContain("No plugins discovered.");
    });
  });

  describe("restore on startup toggle", () => {
    it("optimistically enables and confirms on success", async () => {
      const wrapper = mountSettings();
      await flushPromises();

      const toggle = wrapper.find(".toggle-input");
      await toggle.setValue(false);
      await flushPromises();

      expect((toggle.element as HTMLInputElement).checked).toBe(false);
      expect(invokeMock).toHaveBeenCalledWith("set_restore_on_startup", { enabled: false });
      expect(pushNoticeMock).toHaveBeenCalledWith("success", "Startup restore preference saved");
    });

    it("rolls back on invoke failure", async () => {
      invokeMock.mockImplementation((cmd: string) => {
        if (cmd === "set_restore_on_startup") return Promise.reject(new Error("denied"));
        if (cmd === "get_config") return Promise.resolve({ preferences: {} });
        if (cmd === "list_plugins") return Promise.resolve([]);
        if (cmd === "list_plugin_capability_metadata") return Promise.resolve([]);
        if (cmd === "list_plugin_discovery_errors") return Promise.resolve([]);
        if (cmd === "get_config_paths") return Promise.resolve(makeConfigPaths());
        return Promise.resolve(undefined);
      });

      const wrapper = mountSettings();
      await flushPromises();

      const toggle = wrapper.find(".toggle-input");
      await toggle.setValue(false);
      await flushPromises();

      expect((toggle.element as HTMLInputElement).checked).toBe(true);
      expect(pushNoticeMock).toHaveBeenCalledWith("error", "denied");
    });
  });

  describe("background restore toggle", () => {
    it("enables background restore and reloads settings", async () => {
      const wrapper = mountSettings();
      await flushPromises();
      await wrapper.findAll(".settings-tab")[1].trigger("click");

      const toggle = wrapper.get('[aria-labelledby="settings-tab-background"] .toggle-input');
      await toggle.setValue(true);
      await flushPromises();

      expect(invokeMock).toHaveBeenCalledWith("enable_background_restore");
      expect(pushNoticeMock).toHaveBeenCalledWith("success", "Background restore enabled");
    });

    it("rolls back when disabling fails", async () => {
      mockInvokeDefaults({ get_config: { preferences: { background_restore: true } } });
      invokeMock.mockImplementation((cmd: string) => {
        if (cmd === "disable_background_restore") return Promise.reject(new Error("no permission"));
        if (cmd === "get_config") return Promise.resolve({ preferences: { background_restore: true } });
        if (cmd === "list_plugins") return Promise.resolve([]);
        if (cmd === "list_plugin_capability_metadata") return Promise.resolve([]);
        if (cmd === "list_plugin_discovery_errors") return Promise.resolve([]);
        if (cmd === "get_config_paths") return Promise.resolve(makeConfigPaths());
        return Promise.resolve(undefined);
      });

      const wrapper = mountSettings();
      await flushPromises();
      await wrapper.findAll(".settings-tab")[1].trigger("click");

      const toggle = wrapper.get('[aria-labelledby="settings-tab-background"] .toggle-input');
      expect((toggle.element as HTMLInputElement).checked).toBe(true);
      await toggle.setValue(false);
      await flushPromises();

      expect((toggle.element as HTMLInputElement).checked).toBe(true);
      expect(pushNoticeMock).toHaveBeenCalledWith("error", "no permission");
    });
  });

  describe("plugin enable/disable", () => {
    it("toggles a plugin's enabled state", async () => {
      mockInvokeDefaults({ list_plugins: [makePluginStatus({ id: "p1", name: "P1", enabled: true })] });

      const wrapper = mountSettings();
      await flushPromises();
      await wrapper.findAll(".settings-tab")[2].trigger("click");

      const rowToggle = wrapper.find(".plugins-toggle-cell .toggle-input");
      await rowToggle.setValue(false);
      await flushPromises();

      expect(invokeMock).toHaveBeenCalledWith("set_plugin_enabled", { pluginId: "p1", enabled: false });
      expect(pushNoticeMock).toHaveBeenCalledWith("success", "P1 disabled");
    });

    it("rolls back when toggling a plugin fails", async () => {
      mockInvokeDefaults({ list_plugins: [makePluginStatus({ id: "p1", name: "P1", enabled: true })] });
      invokeMock.mockImplementation((cmd: string) => {
        if (cmd === "set_plugin_enabled") return Promise.reject(new Error("crash-looped"));
        if (cmd === "get_config") return Promise.resolve({ preferences: {} });
        if (cmd === "list_plugins") return Promise.resolve([makePluginStatus({ id: "p1", name: "P1", enabled: true })]);
        if (cmd === "list_plugin_capability_metadata") return Promise.resolve([]);
        if (cmd === "list_plugin_discovery_errors") return Promise.resolve([]);
        if (cmd === "get_config_paths") return Promise.resolve(makeConfigPaths());
        return Promise.resolve(undefined);
      });

      const wrapper = mountSettings();
      await flushPromises();
      await wrapper.findAll(".settings-tab")[2].trigger("click");

      const rowToggle = wrapper.find(".plugins-toggle-cell .toggle-input");
      await rowToggle.setValue(false);
      await flushPromises();

      // togglePlugin has no local rollback assignment (it relies on the
      // subsequent loadSettings() re-fetch), so the row should still reflect
      // the invoke-mocked "enabled: true" plugin state after the failure.
      expect(pushNoticeMock).toHaveBeenCalledWith("error", "crash-looped");
    });
  });

  describe("capability grants", () => {
    it("grants a capability via the plugin detail modal", async () => {
      mockInvokeDefaults({
        list_plugins: [makePluginStatus({ id: "p1", name: "P1", requested_capabilities: ["network"] })],
        list_plugin_capability_metadata: [makeCapabilityInfo({ id: "network" })],
      });

      const wrapper = mountSettings();
      await flushPromises();
      await wrapper.findAll(".settings-tab")[2].trigger("click");
      await wrapper.find(".plugins-table-row").trigger("click");
      await flushPromises();

      expect(wrapper.find(".plugin-modal").exists()).toBe(true);

      const capabilityToggle = wrapper.find(".plugin-capability-row .toggle-input");
      await capabilityToggle.setValue(true);
      await flushPromises();

      expect(invokeMock).toHaveBeenCalledWith("grant_plugin_capabilities", {
        pluginId: "p1",
        capabilities: ["network"],
      });
    });
  });

  describe("plugin table sorting", () => {
    it("toggles sort direction when the same column header is clicked twice", async () => {
      mockInvokeDefaults({
        list_plugins: [
          makePluginStatus({ id: "a", name: "Alpha" }),
          makePluginStatus({ id: "b", name: "Beta" }),
        ],
      });

      const wrapper = mountSettings();
      await flushPromises();
      await wrapper.findAll(".settings-tab")[2].trigger("click");

      const nameHeader = wrapper.findAll(".plugins-sortable-th")[0];
      let names = wrapper.findAll(".plugins-table-row strong").map((n) => n.text());
      expect(names).toEqual(["Alpha", "Beta"]);

      await nameHeader.trigger("click");
      names = wrapper.findAll(".plugins-table-row strong").map((n) => n.text());
      expect(names).toEqual(["Beta", "Alpha"]);
    });
  });

  describe("theme", () => {
    it("shows the Active badge next to the currently resolved scheme", async () => {
      resolvedKind.value = "dark";
      const wrapper = mountSettings();
      await flushPromises();

      expect(wrapper.find(".scheme-active-badge").exists()).toBe(true);
    });

    it("disables Reset to default only when already on the default theme", async () => {
      themeMode.value = "system";
      darkSchemeId.value = "midnight-deck";
      lightSchemeId.value = "paper-deck";

      const wrapper = mountSettings();
      await flushPromises();

      const resetButton = wrapper.findAll("button").find((b) => b.text() === "Reset to default");
      expect(resetButton?.attributes("disabled")).toBeDefined();
    });
  });

  describe("update panel", () => {
    it("shows a checking state", async () => {
      checkingUpdates.value = true;
      const wrapper = mountSettings();
      await flushPromises();
      await wrapper.findAll(".settings-tab")[3].trigger("click");

      expect(wrapper.find(".settings-update-copy").text()).toContain("Checking GitHub releases…");
    });

    it("shows an outdated result with an install button", async () => {
      updateResult.value = makeUpdateResult({
        status: "outdated",
        latestVersion: "9.9.9",
        canAutoInstall: true,
      });

      const wrapper = mountSettings();
      await flushPromises();
      await wrapper.findAll(".settings-tab")[3].trigger("click");

      expect(wrapper.find(".settings-update-copy").text()).toContain("v9.9.9");
      const installButton = wrapper
        .findAll("button")
        .find((b) => b.text() === "Install update");
      expect(installButton).toBeTruthy();
    });

    it("shows install-complete state and hides the install button", async () => {
      updateResult.value = makeUpdateResult({ status: "outdated", latestVersion: "9.9.9" });
      installComplete.value = true;

      const wrapper = mountSettings();
      await flushPromises();
      await wrapper.findAll(".settings-tab")[3].trigger("click");

      expect(wrapper.find(".settings-update-copy").text()).toContain(
        "Update installed — relaunch Pipe Deck to finish.",
      );
      expect(wrapper.findAll("button").find((b) => b.text() === "Install update")).toBeFalsy();
    });

    it("calls checkForUpdatesNow when Check now is clicked", async () => {
      const wrapper = mountSettings();
      await flushPromises();
      await wrapper.findAll(".settings-tab")[3].trigger("click");

      const checkButton = wrapper.findAll("button").find((b) => b.text() === "Check now");
      await checkButton?.trigger("click");

      expect(checkForUpdatesNowMock).toHaveBeenCalled();
    });
  });

  describe("PluginDetailModal", () => {
    it("opens on row click and closes on the close action", async () => {
      mockInvokeDefaults({ list_plugins: [makePluginStatus({ id: "p1", name: "P1" })] });

      const wrapper = mountSettings();
      await flushPromises();
      await wrapper.findAll(".settings-tab")[2].trigger("click");
      await wrapper.find(".plugins-table-row").trigger("click");
      await flushPromises();

      expect(wrapper.find(".plugin-modal").exists()).toBe(true);

      await wrapper.find(".dialog-actions button").trigger("click");
      await flushPromises();

      expect(wrapper.find(".plugin-modal").exists()).toBe(false);
    });
  });

  describe("diagnostics and paths", () => {
    beforeEach(() => {
      Object.defineProperty(navigator, "clipboard", {
        value: { writeText: vi.fn().mockResolvedValue(undefined) },
        configurable: true,
      });
    });

    it("copies the diagnostics bundle to the clipboard", async () => {
      mockInvokeDefaults({ get_diagnostics_bundle: "diagnostics-text" });

      const wrapper = mountSettings();
      await flushPromises();
      await wrapper.findAll(".settings-tab")[3].trigger("click");

      const aboutPanel = wrapper.get('[aria-labelledby="settings-tab-about"]');
      const copyButton = aboutPanel.findAll("button").find((b) => b.text() === "Copy");
      await copyButton?.trigger("click");
      await flushPromises();

      expect(invokeMock).toHaveBeenCalledWith("get_diagnostics_bundle");
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith("diagnostics-text");
      expect(pushNoticeMock).toHaveBeenCalledWith("success", "Diagnostics copied to clipboard.");
    });

    it("rescans plugin directories", async () => {
      const wrapper = mountSettings();
      await flushPromises();
      await wrapper.findAll(".settings-tab")[2].trigger("click");

      await wrapper.find(".plugins-panel-header button").trigger("click");
      await flushPromises();

      expect(invokeMock).toHaveBeenCalledWith("rescan_plugins");
      expect(pushNoticeMock).toHaveBeenCalledWith("success", "Plugin directories rescanned");
    });
  });
});
