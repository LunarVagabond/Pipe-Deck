import { describe, expect, it, vi, beforeEach } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ setTheme: vi.fn() }),
}));
vi.mock("./notices", () => ({
  useApplyResult: () => ({ handleApplyResult: vi.fn() }),
}));

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(undefined);
});

describe("theme store", () => {
  it("resets appearance preferences to the documented default schemes", async () => {
    const { useTheme } = await import("./theme");
    const theme = useTheme();

    await theme.setMode("dark");
    await theme.setDarkScheme("custom-dark");
    await theme.setLightScheme("custom-light");
    invokeMock.mockClear();

    await theme.resetToDefaults();

    expect(theme.mode.value).toBe("system");
    expect(theme.darkSchemeId.value).toBe("midnight-deck");
    expect(theme.lightSchemeId.value).toBe("paper-deck");
    expect(invokeMock).toHaveBeenNthCalledWith(1, "set_theme_mode", { mode: "system" });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "set_dark_scheme", { id: "midnight-deck" });
    expect(invokeMock).toHaveBeenNthCalledWith(3, "set_light_scheme", { id: "paper-deck" });
  });
});
