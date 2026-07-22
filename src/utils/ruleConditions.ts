import type { RuleCondition, Stream, StreamDirection } from "../types/graph";

export type ConditionType = RuleCondition["type"];

export interface ConditionTypeMeta {
  type: ConditionType;
  label: string;
  description: string;
  example: string;
  placeholder: string;
}

export const CONDITION_TYPE_OPTIONS: ConditionTypeMeta[] = [
  {
    type: "identity",
    label: "App identity",
    description:
      "Matches app name, executable, or PipeWire node name (case-insensitive). Use this for rules like “pw-play” or “firefox”.",
    example: "pw-play",
    placeholder: "e.g. pw-play",
  },
  {
    type: "executable",
    label: "Executable",
    description:
      "The process binary on disk. Usually the most reliable way to match an app across distros.",
    example: "discord",
    placeholder: "e.g. discord",
  },
  {
    type: "app_name",
    label: "App Name",
    description:
      "The friendly application name reported by PipeWire. Can vary between installs or app builds.",
    example: "Discord",
    placeholder: "e.g. Discord",
  },
  {
    type: "media_name",
    label: "Media Name",
    description:
      "Optional stream label inside an app. Useful when one app has multiple audio streams.",
    example: "miniaudio",
    placeholder: "e.g. miniaudio",
  },
  {
    type: "window_class",
    label: "Window Class",
    description:
      "Best-effort desktop identity from PipeWire metadata (X11 class when present, otherwise application.id or icon name). May be unavailable on some Wayland setups.",
    example: "firefox",
    placeholder: "e.g. firefox",
  },
  {
    type: "direction",
    label: "Direction",
    description: "Whether the stream is playback output or capture input.",
    example: "playback",
    placeholder: "playback or capture",
  },
  {
    type: "category",
    label: "Category",
    description: "A built-in grouping such as Game, Music, or Browser inferred from the app.",
    example: "Game",
    placeholder: "e.g. Game",
  },
  {
    type: "regex",
    label: "Regex",
    description: "Advanced pattern match against a specific identity field.",
    example: "/Custom.*/ on App Name",
    placeholder: "pattern",
  },
];

export const DIRECTION_OPTIONS: { value: StreamDirection; label: string }[] = [
  { value: "playback", label: "Playback" },
  { value: "capture", label: "Capture" },
];

export const CATEGORY_OPTIONS = ["Game", "Music", "Chat", "Browser", "Streaming"] as const;

export const REGEX_FIELD_OPTIONS: { value: string; label: string }[] = [
  { value: "app_name", label: "App Name" },
  { value: "executable", label: "Executable" },
  { value: "media_name", label: "Media Name" },
  { value: "window_class", label: "Window Class" },
];

export function conditionTypeLabel(type: ConditionType): string {
  return CONDITION_TYPE_OPTIONS.find((entry) => entry.type === type)?.label ?? type;
}

export function conditionTypeMeta(type: ConditionType): ConditionTypeMeta {
  return (
    CONDITION_TYPE_OPTIONS.find((entry) => entry.type === type) ?? {
      type,
      label: type,
      description: "",
      example: "",
      placeholder: "value",
    }
  );
}

export function streamFieldValue(stream: Stream, type: ConditionType): string | undefined {
  switch (type) {
    case "identity":
      return stream.executable ?? (stream.app_name || undefined);
    case "app_name":
      return stream.app_name || undefined;
    case "executable":
      return stream.executable;
    case "media_name":
      return stream.media_name;
    case "window_class":
      return stream.window_class;
    case "direction":
      return stream.direction;
    case "category":
      return inferStreamCategory(stream);
    default:
      return undefined;
  }
}

export function inferStreamCategory(stream: Stream): string | undefined {
  const executable = stream.executable?.toLowerCase() ?? "";
  const appLower = stream.app_name.toLowerCase();

  if (executable.includes("steam") || appLower.includes("steam")) return "Game";
  if (executable.includes("spotify") || appLower.includes("spotify")) return "Music";
  if (executable.includes("discord") || appLower.includes("discord")) return "Chat";
  if (
    executable.includes("firefox") ||
    executable.includes("chromium") ||
    executable.includes("chrome") ||
    appLower.includes("firefox") ||
    appLower.includes("chromium")
  ) {
    return "Browser";
  }
  if (executable.includes("obs") || appLower.includes("obs")) return "Streaming";
  return undefined;
}

export function liveSuggestionsForType(
  streams: Stream[],
  type: ConditionType,
): string[] {
  const values = new Set<string>();
  for (const stream of streams) {
    const value = streamFieldValue(stream, type);
    if (value?.trim()) {
      values.add(value.trim());
    }
  }
  return [...values].sort((left, right) => left.localeCompare(right));
}

export function conditionValue(condition: RuleCondition): string {
  if (condition.type === "regex") {
    return condition.pattern;
  }
  return condition.value;
}

export function setConditionValue(condition: RuleCondition, value: string) {
  if (condition.type === "regex") {
    condition.pattern = value;
    return;
  }
  condition.value = value;
}

export function setConditionType(condition: RuleCondition, type: ConditionType) {
  if (type === "regex") {
    Object.assign(condition, { type, field: "app_name", pattern: "" });
    return;
  }
  if (type === "direction") {
    Object.assign(condition, { type, value: "playback" });
    return;
  }
  if (type === "category") {
    Object.assign(condition, { type, value: "Game" });
    return;
  }
  Object.assign(condition, { type, value: "" });
}

export function formatConditionSummary(condition: RuleCondition): string {
  if (condition.type === "regex") {
    const field = REGEX_FIELD_OPTIONS.find((entry) => entry.value === condition.field)?.label
      ?? condition.field;
    return `${field} matches /${condition.pattern}/`;
  }
  if (condition.type === "direction") {
    return condition.value === "capture" ? "Capture" : "Playback";
  }
  if (condition.type === "identity") {
    return condition.value;
  }
  return condition.value;
}
