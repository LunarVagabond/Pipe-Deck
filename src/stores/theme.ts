import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useApplyResult } from "./notices";
import type { Preferences } from "../types/graph";
import type { ResolvedScheme, ThemeBaseKind, ThemeMode } from "../types/theme";

export const DEFAULT_DARK_SCHEME_ID = "midnight-deck";
export const DEFAULT_LIGHT_SCHEME_ID = "paper-deck";
export const DEFAULT_THEME_MODE: ThemeMode = "system";

// Module-level singleton so Settings and the app boot sequence share one applied theme.
const schemes = ref<ResolvedScheme[]>([]);
const mode = ref<ThemeMode>("dark");
const darkSchemeId = ref(DEFAULT_DARK_SCHEME_ID);
const lightSchemeId = ref(DEFAULT_LIGHT_SCHEME_ID);
const systemPrefersDark = ref(true);
let mediaQueryAttached = false;

const resolvedKind = computed<ThemeBaseKind>(() => {
  if (mode.value === "system") return systemPrefersDark.value ? "dark" : "light";
  return mode.value;
});

const activeScheme = computed<ResolvedScheme | null>(() => {
  const wantedId = resolvedKind.value === "dark" ? darkSchemeId.value : lightSchemeId.value;
  const fallbackId = resolvedKind.value === "dark" ? DEFAULT_DARK_SCHEME_ID : DEFAULT_LIGHT_SCHEME_ID;
  return (
    schemes.value.find((scheme) => scheme.id === wantedId) ??
    schemes.value.find((scheme) => scheme.id === fallbackId) ??
    null
  );
});

function applyToDom() {
  const scheme = activeScheme.value;
  if (!scheme) return;
  const root = document.documentElement.style;
  const colors = scheme.colors;
  root.setProperty("--background", colors.background);
  root.setProperty("--surface-1", colors.surface_1);
  root.setProperty("--surface-2", colors.surface_2);
  root.setProperty("--border", colors.border);
  root.setProperty("--text", colors.text);
  root.setProperty("--text-muted", colors.text_muted);
  root.setProperty("--accent-purple", colors.accent_purple);
  root.setProperty("--accent-teal", colors.accent_teal);
  root.setProperty("--accent-amber", colors.accent_amber);
  root.setProperty("--status-success", colors.status_success);
  root.setProperty("--status-warning", colors.status_warning);
  root.setProperty("--status-danger", colors.status_danger);
  root.setProperty("color", colors.text);
  root.setProperty("background-color", colors.background);
  // Tells the browser to render native controls (select popups, scrollbars,
  // checkboxes) using a matching palette instead of defaulting to light UA
  // chrome on a dark page (or vice versa) — fixes unreadable native dropdowns.
  root.setProperty("color-scheme", scheme.kind);
  void applyToWindowChrome(scheme.kind);
}

// Best-effort: hints the native window (title bar, and on Linux the CSD
// minimize/maximize/close controls) to render in the matching palette via
// Tauri's cross-platform Window.setTheme. No-op outside a real Tauri window
// (plain browser dev, or platforms — e.g. a future mobile port — where the
// concept of a themeable native title bar doesn't apply), so this is wrapped
// defensively rather than gated by an explicit OS check.
async function applyToWindowChrome(kind: ThemeBaseKind) {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().setTheme(kind);
  } catch {
    // Not running inside a Tauri window (e.g. `vite dev` in a browser) — ignore.
  }
}

export function useTheme() {
  const { handleApplyResult } = useApplyResult();

  function attachSystemThemeListener() {
    if (mediaQueryAttached || typeof window === "undefined" || !window.matchMedia) return;
    mediaQueryAttached = true;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    systemPrefersDark.value = media.matches;
    media.addEventListener("change", (event) => {
      systemPrefersDark.value = event.matches;
      applyToDom();
    });
  }

  async function initTheme() {
    attachSystemThemeListener();
    try {
      const config = await invoke<{ preferences?: Preferences }>("get_config");
      mode.value = (config.preferences?.theme_mode as ThemeMode) ?? "dark";
      darkSchemeId.value = config.preferences?.dark_scheme ?? DEFAULT_DARK_SCHEME_ID;
      lightSchemeId.value = config.preferences?.light_scheme ?? DEFAULT_LIGHT_SCHEME_ID;
      schemes.value = await invoke<ResolvedScheme[]>("list_themes");
    } catch {
      // Static _variables.scss dark palette stands as the fallback if theme load fails.
    }
    applyToDom();
  }

  // These three setters apply instantly and visibly (the whole UI recolors),
  // so a success toast on top would be redundant noise — only surface a
  // notice when something actually goes wrong and the change gets rolled back.
  async function setMode(next: ThemeMode) {
    const previous = mode.value;
    mode.value = next;
    applyToDom();
    try {
      await invoke("set_theme_mode", { mode: next });
    } catch (error) {
      mode.value = previous;
      applyToDom();
      handleApplyResult(
        { success: false, message: error instanceof Error ? error.message : String(error) },
        "",
      );
    }
  }

  async function setDarkScheme(id: string) {
    const previous = darkSchemeId.value;
    darkSchemeId.value = id;
    applyToDom();
    try {
      await invoke("set_dark_scheme", { id });
    } catch (error) {
      darkSchemeId.value = previous;
      applyToDom();
      handleApplyResult(
        { success: false, message: error instanceof Error ? error.message : String(error) },
        "",
      );
    }
  }

  async function setLightScheme(id: string) {
    const previous = lightSchemeId.value;
    lightSchemeId.value = id;
    applyToDom();
    try {
      await invoke("set_light_scheme", { id });
    } catch (error) {
      lightSchemeId.value = previous;
      applyToDom();
      handleApplyResult(
        { success: false, message: error instanceof Error ? error.message : String(error) },
        "",
      );
    }
  }

  async function resetToDefaults() {
    await setMode(DEFAULT_THEME_MODE);
    await setDarkScheme(DEFAULT_DARK_SCHEME_ID);
    await setLightScheme(DEFAULT_LIGHT_SCHEME_ID);
  }

  return {
    schemes,
    mode,
    darkSchemeId,
    lightSchemeId,
    resolvedKind,
    activeScheme,
    initTheme,
    setMode,
    setDarkScheme,
    setLightScheme,
    resetToDefaults,
  };
}
