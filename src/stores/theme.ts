import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useApplyResult } from "./notices";
import type { Preferences } from "../types/graph";
import type { ResolvedScheme, ThemeBaseKind, ThemeMode } from "../types/theme";

const FALLBACK_DARK_ID = "midnight-deck";
const FALLBACK_LIGHT_ID = "paper-deck";

// Module-level singleton so Settings and the app boot sequence share one applied theme.
const schemes = ref<ResolvedScheme[]>([]);
const mode = ref<ThemeMode>("dark");
const darkSchemeId = ref(FALLBACK_DARK_ID);
const lightSchemeId = ref(FALLBACK_LIGHT_ID);
const systemPrefersDark = ref(true);
let mediaQueryAttached = false;

const resolvedKind = computed<ThemeBaseKind>(() => {
  if (mode.value === "system") return systemPrefersDark.value ? "dark" : "light";
  return mode.value;
});

const activeScheme = computed<ResolvedScheme | null>(() => {
  const wantedId = resolvedKind.value === "dark" ? darkSchemeId.value : lightSchemeId.value;
  const fallbackId = resolvedKind.value === "dark" ? FALLBACK_DARK_ID : FALLBACK_LIGHT_ID;
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
  root.setProperty("color", colors.text);
  root.setProperty("background-color", colors.background);
  // Tells the browser to render native controls (select popups, scrollbars,
  // checkboxes) using a matching palette instead of defaulting to light UA
  // chrome on a dark page (or vice versa) — fixes unreadable native dropdowns.
  root.setProperty("color-scheme", scheme.kind);
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
      darkSchemeId.value = config.preferences?.dark_scheme ?? FALLBACK_DARK_ID;
      lightSchemeId.value = config.preferences?.light_scheme ?? FALLBACK_LIGHT_ID;
      schemes.value = await invoke<ResolvedScheme[]>("list_themes");
    } catch {
      // Static _variables.scss dark palette stands as the fallback if theme load fails.
    }
    applyToDom();
  }

  async function setMode(next: ThemeMode) {
    const previous = mode.value;
    mode.value = next;
    applyToDom();
    try {
      await invoke("set_theme_mode", { mode: next });
      handleApplyResult({ success: true }, "Appearance mode saved");
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
      handleApplyResult({ success: true }, "Dark scheme saved");
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
      handleApplyResult({ success: true }, "Light scheme saved");
    } catch (error) {
      lightSchemeId.value = previous;
      applyToDom();
      handleApplyResult(
        { success: false, message: error instanceof Error ? error.message : String(error) },
        "",
      );
    }
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
  };
}
