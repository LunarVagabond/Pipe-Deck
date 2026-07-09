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
      "The desktop window class when available. Helpful when executable and app name are ambiguous.",
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

export function formatConditionSummary(condition: RuleCondition): string {
  if (condition.type === "regex") {
    const field = REGEX_FIELD_OPTIONS.find((entry) => entry.value === condition.field)?.label
      ?? condition.field;
    return `${field} matches /${condition.pattern}/`;
  }
  if (condition.type === "direction") {
    return condition.value === "capture" ? "Capture" : "Playback";
  }
  return condition.value;
}
