// The bundled effects plugin (#209) predates the native "effects as attachments"
// model (PD-020/024/025) and no longer gates anything — its enabled state isn't
// read anywhere in the effects UI or engine. Its toggle is disabled in Settings
// rather than functional, so it can't misleadingly suggest it turns effects off.
export const EFFECTS_PLUGIN_ID = "pipe-deck-effects";

export function isAlwaysOnPlugin(pluginId: string): boolean {
  return pluginId === EFFECTS_PLUGIN_ID;
}
