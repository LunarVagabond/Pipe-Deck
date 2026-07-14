export type ThemeMode = "light" | "dark" | "system";
export type ThemeBaseKind = "light" | "dark";
export type ThemeSchemeSource = "builtin" | "custom";

export interface ThemeColors {
  background: string;
  surface_1: string;
  surface_2: string;
  border: string;
  text: string;
  text_muted: string;
  accent_purple: string;
  accent_teal: string;
  accent_amber: string;
}

export interface ResolvedScheme {
  id: string;
  name: string;
  kind: ThemeBaseKind;
  source: ThemeSchemeSource;
  colors: ThemeColors;
}
