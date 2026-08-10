import { describe, expect, it, vi, beforeEach } from "vitest";
import {
  compareUpdateStatus,
  platformKeyForInstallKind,
  fetchUpdateManifest,
  fetchNewestReleaseTag,
  manifestUrlForTag,
  releasePageForTag,
  checkForUpdates,
  installUpdate,
  UPDATE_MANIFEST_URL,
  GITHUB_RELEASES_API_URL,
  RELEASES_PAGE,
  updateStatusLabel,
} from "./updates";
import type { AppInfo, InstallKind, UpdateCheckResult } from "../types/app";

const invokeMock = vi.hoisted(() => vi.fn());
const fetchMock = vi.hoisted(() => vi.fn());
const checkUpdaterMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/plugin-http", () => ({ fetch: fetchMock }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: checkUpdaterMock }));

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(undefined);
  fetchMock.mockReset();
  checkUpdaterMock.mockReset();
});

function appInfo(overrides: Partial<AppInfo> = {}): AppInfo {
  return {
    buildRevision: "abc123",
    releaseVersion: "1.2.0",
    installKind: "app_image",
    installLabel: "AppImage",
    ...overrides,
  };
}

describe("compareUpdateStatus", () => {
  it("reports current when latest is behind current", () => {
    expect(compareUpdateStatus("1.2.0", "1.1.0")).toBe("current");
  });

  it("reports current when versions are exactly equal", () => {
    expect(compareUpdateStatus("1.2.0", "1.2.0")).toBe("current");
  });

  it("reports outdated exactly one minor version ahead", () => {
    expect(compareUpdateStatus("1.2.0", "1.3.0")).toBe("outdated");
  });

  it("reports severely_outdated two or more minor versions ahead", () => {
    expect(compareUpdateStatus("1.2.0", "1.4.0")).toBe("severely_outdated");
  });

  it("reports severely_outdated on any major version bump", () => {
    expect(compareUpdateStatus("1.9.0", "2.0.0")).toBe("severely_outdated");
  });

  it("tolerates malformed version strings via the parseInt fallback", () => {
    expect(compareUpdateStatus("1.2.3", "v1.2.3")).toBe("current");
    expect(compareUpdateStatus("1.0.0", "1.x.0")).toBe("current");
  });
});

describe("platformKeyForInstallKind", () => {
  it.each([
    ["app_image", "linux-x86_64-appimage"],
    ["deb", "linux-x86_64-deb"],
    ["rpm", "linux-x86_64-rpm"],
    ["native", "linux-x86_64-binary"],
    ["dev", "linux-x86_64-binary"],
  ] satisfies [InstallKind, string][])("maps %s to %s", (kind, expected) => {
    expect(platformKeyForInstallKind(kind)).toBe(expected);
  });

  it("returns null for an unrecognized install kind", () => {
    expect(platformKeyForInstallKind("unknown" as InstallKind)).toBeNull();
  });
});

describe("fetchUpdateManifest", () => {
  it("fetches the manifest URL and returns the parsed body on success", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ version: "1.3.0", platforms: {} }),
    });

    const manifest = await fetchUpdateManifest();

    expect(fetchMock).toHaveBeenCalledWith(UPDATE_MANIFEST_URL, {
      headers: { Accept: "application/json" },
    });
    expect(manifest).toEqual({ version: "1.3.0", platforms: {} });
  });

  it("throws with the response status when the fetch is not ok", async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 404 });

    await expect(fetchUpdateManifest()).rejects.toThrow("404");
  });

  it("fetches an arbitrary manifest URL when one is passed explicitly", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ version: "1.4.0-preview", platforms: {} }),
    });

    const manifest = await fetchUpdateManifest(
      "https://example.test/latest.json",
    );

    expect(fetchMock).toHaveBeenCalledWith("https://example.test/latest.json", {
      headers: { Accept: "application/json" },
    });
    expect(manifest.version).toBe("1.4.0-preview");
  });
});

describe("manifestUrlForTag / releasePageForTag", () => {
  it("builds the per-tag latest.json download URL", () => {
    expect(manifestUrlForTag("v1.4.0-preview")).toBe(
      "https://github.com/LunarVagabond/Pipe-Deck/releases/download/v1.4.0-preview/latest.json",
    );
  });

  it("builds the per-tag release page URL", () => {
    expect(releasePageForTag("v1.4.0-preview")).toBe(
      "https://github.com/LunarVagabond/Pipe-Deck/releases/tag/v1.4.0-preview",
    );
  });
});

describe("fetchNewestReleaseTag", () => {
  it("fetches the GitHub releases API and returns the newest non-draft release", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve([
          {
            tag_name: "v1.4.0-preview",
            draft: false,
            html_url: "https://example.test/v1.4.0-preview",
          },
          {
            tag_name: "v1.3.0",
            draft: false,
            html_url: "https://example.test/v1.3.0",
          },
        ]),
    });

    const release = await fetchNewestReleaseTag();

    expect(fetchMock).toHaveBeenCalledWith(GITHUB_RELEASES_API_URL, {
      headers: { Accept: "application/vnd.github+json" },
    });
    expect(release.tag_name).toBe("v1.4.0-preview");
  });

  it("skips draft releases when picking the newest", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve([
          {
            tag_name: "v1.5.0-draft",
            draft: true,
            html_url: "https://example.test/v1.5.0-draft",
          },
          {
            tag_name: "v1.4.0-preview",
            draft: false,
            html_url: "https://example.test/v1.4.0-preview",
          },
        ]),
    });

    const release = await fetchNewestReleaseTag();

    expect(release.tag_name).toBe("v1.4.0-preview");
  });

  it("throws when no non-draft releases exist", async () => {
    fetchMock.mockResolvedValue({ ok: true, json: () => Promise.resolve([]) });

    await expect(fetchNewestReleaseTag()).rejects.toThrow(
      "No published releases found",
    );
  });

  it("throws with the response status when the releases lookup is not ok", async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 403 });

    await expect(fetchNewestReleaseTag()).rejects.toThrow("403");
  });
});

describe("checkForUpdates", () => {
  it("reports the latest release as a dev_build result when there is no tagged release version", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ version: "1.3.0", platforms: {} }),
    });

    const result = await checkForUpdates(
      appInfo({ releaseVersion: undefined }),
    );

    expect(result).toEqual({
      status: "dev_build",
      currentVersion: "abc123",
      latestVersion: "1.3.0",
      releaseUrl: RELEASES_PAGE,
      canAutoInstall: false,
    });
  });

  it("reports an error with the build revision when the manifest fetch fails and there is no tagged release version", async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 500 });

    const result = await checkForUpdates(
      appInfo({ releaseVersion: undefined }),
    );

    expect(result.status).toBe("error");
    expect(result.currentVersion).toBe("abc123");
    expect(result.error).toContain("500");
  });

  it("resolves status/downloadUrl on the happy path for an app_image install", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          version: "1.3.0",
          platforms: {
            "linux-x86_64-appimage": {
              url: "https://example.test/app.AppImage",
              signature: "sig",
            },
          },
        }),
    });

    const result = await checkForUpdates(appInfo());

    expect(result.status).toBe("outdated");
    expect(result.currentVersion).toBe("1.2.0");
    expect(result.latestVersion).toBe("1.3.0");
    expect(result.releaseUrl).toBe(RELEASES_PAGE);
    expect(result.downloadUrl).toBe("https://example.test/app.AppImage");
    expect(result.canAutoInstall).toBe(true);
    expect(result.error).toBeUndefined();
  });

  it("does not allow auto-install for a non-app_image install kind even with a matching platform", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          version: "1.3.0",
          platforms: {
            "linux-x86_64-deb": {
              url: "https://example.test/app.deb",
              signature: "sig",
            },
          },
        }),
    });

    const result = await checkForUpdates(appInfo({ installKind: "deb" }));

    expect(result.canAutoInstall).toBe(false);
    expect(result.downloadUrl).toBe("https://example.test/app.deb");
  });

  it("does not allow auto-install for app_image when the platform has no signature", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          version: "1.3.0",
          platforms: {
            "linux-x86_64-appimage": {
              url: "https://example.test/app.AppImage",
            },
          },
        }),
    });

    const result = await checkForUpdates(appInfo());

    expect(result.canAutoInstall).toBe(false);
  });

  it("errors when the manifest has no version", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ version: "", platforms: {} }),
    });

    const result = await checkForUpdates(appInfo());

    expect(result.status).toBe("error");
    expect(result.error).toBe("Update manifest has no version");
  });

  it("errors when the platform key resolves but the manifest has no matching platform entry", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ version: "1.3.0", platforms: {} }),
    });

    const result = await checkForUpdates(appInfo());

    expect(result.downloadUrl).toBeUndefined();
    expect(result.error).toBe(
      "No packaged download for this install type yet — use the releases page instead.",
    );
  });

  it("does not raise a missing-platform error for an unrecognized install kind", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ version: "1.3.0", platforms: {} }),
    });

    const result = await checkForUpdates(
      appInfo({ installKind: "unknown" as InstallKind }),
    );

    expect(result.downloadUrl).toBeUndefined();
    expect(result.error).toBeUndefined();
  });

  it("catches a manifest fetch failure and reports it as an error result", async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 500 });

    const result = await checkForUpdates(appInfo());

    expect(result.status).toBe("error");
    expect(result.error).toContain("500");
    expect(result.canAutoInstall).toBe(false);
  });
});

describe("checkForUpdates with the prerelease channel", () => {
  it("looks up the newest release tag, then fetches that release's own latest.json", async () => {
    fetchMock
      .mockResolvedValueOnce({
        ok: true,
        json: () =>
          Promise.resolve([
            {
              tag_name: "v1.4.0-preview",
              draft: false,
              html_url: "https://example.test/releases/v1.4.0-preview",
            },
          ]),
      })
      .mockResolvedValueOnce({
        ok: true,
        json: () =>
          Promise.resolve({
            version: "1.4.0-preview",
            platforms: {
              "linux-x86_64-appimage": {
                url: "https://example.test/app.AppImage",
                signature: "sig",
              },
            },
          }),
      });

    const result = await checkForUpdates(appInfo(), "prerelease");

    expect(fetchMock).toHaveBeenNthCalledWith(1, GITHUB_RELEASES_API_URL, {
      headers: { Accept: "application/vnd.github+json" },
    });
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "https://github.com/LunarVagabond/Pipe-Deck/releases/download/v1.4.0-preview/latest.json",
      { headers: { Accept: "application/json" } },
    );
    expect(result.status).toBe("severely_outdated");
    expect(result.latestVersion).toBe("1.4.0-preview");
    expect(result.releaseUrl).toBe(
      "https://example.test/releases/v1.4.0-preview",
    );
  });

  it("never allows auto-install, even for an app_image install with a signed platform entry", async () => {
    fetchMock
      .mockResolvedValueOnce({
        ok: true,
        json: () =>
          Promise.resolve([
            {
              tag_name: "v1.4.0-preview",
              draft: false,
              html_url: "https://example.test/releases/v1.4.0-preview",
            },
          ]),
      })
      .mockResolvedValueOnce({
        ok: true,
        json: () =>
          Promise.resolve({
            version: "1.4.0-preview",
            platforms: {
              "linux-x86_64-appimage": {
                url: "https://example.test/app.AppImage",
                signature: "sig",
              },
            },
          }),
      });

    const result = await checkForUpdates(appInfo(), "prerelease");

    // The Tauri updater plugin's one-click AppImage installer always checks the
    // static stable-channel endpoint baked into tauri.conf.json — it can't be
    // pointed at this prerelease tag, so the manual download-link flow must be
    // used instead of the auto-installer here.
    expect(result.canAutoInstall).toBe(false);
    expect(result.downloadUrl).toBe("https://example.test/app.AppImage");
  });

  it("surfaces the releases lookup failure as an error result", async () => {
    fetchMock.mockResolvedValueOnce({ ok: false, status: 403 });

    const result = await checkForUpdates(appInfo(), "prerelease");

    expect(result.status).toBe("error");
    expect(result.error).toContain("403");
  });

  it("defaults to the latest channel when no channel argument is passed", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ version: "1.3.0", platforms: {} }),
    });

    await checkForUpdates(appInfo());

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock).toHaveBeenCalledWith(UPDATE_MANIFEST_URL, {
      headers: { Accept: "application/json" },
    });
  });
});

describe("installUpdate", () => {
  it("downloads and installs via the updater plugin when auto-install is available", async () => {
    const downloadAndInstall = vi.fn().mockResolvedValue(undefined);
    checkUpdaterMock.mockResolvedValue({ downloadAndInstall });

    await installUpdate({
      status: "outdated",
      currentVersion: "1.2.0",
      canAutoInstall: true,
    });

    expect(checkUpdaterMock).toHaveBeenCalled();
    expect(downloadAndInstall).toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("reports download progress via the onProgress callback", async () => {
    const downloadAndInstall = vi.fn().mockImplementation(async (onEvent) => {
      onEvent({ event: "Started", data: { contentLength: 200 } });
      onEvent({ event: "Progress", data: { chunkLength: 100 } });
      onEvent({ event: "Progress", data: { chunkLength: 100 } });
      onEvent({ event: "Finished" });
    });
    checkUpdaterMock.mockResolvedValue({ downloadAndInstall });
    const onProgress = vi.fn();

    await installUpdate(
      { status: "outdated", currentVersion: "1.2.0", canAutoInstall: true },
      onProgress,
    );

    expect(onProgress.mock.calls.map((call) => call[0])).toEqual([
      0, 50, 99, 100,
    ]);
  });

  it("reports null progress when the download has no known content length", async () => {
    const downloadAndInstall = vi.fn().mockImplementation(async (onEvent) => {
      onEvent({ event: "Started", data: {} });
      onEvent({ event: "Progress", data: { chunkLength: 100 } });
    });
    checkUpdaterMock.mockResolvedValue({ downloadAndInstall });
    const onProgress = vi.fn();

    await installUpdate(
      { status: "outdated", currentVersion: "1.2.0", canAutoInstall: true },
      onProgress,
    );

    expect(onProgress.mock.calls.map((call) => call[0])).toEqual([null, null]);
  });

  it("throws when the updater plugin has no update available", async () => {
    checkUpdaterMock.mockResolvedValue(undefined);

    await expect(
      installUpdate({
        status: "outdated",
        currentVersion: "1.2.0",
        canAutoInstall: true,
      }),
    ).rejects.toThrow("No update available from the updater plugin");
  });

  it("opens the download URL in preference to the release URL", async () => {
    const result: UpdateCheckResult = {
      status: "outdated",
      currentVersion: "1.2.0",
      canAutoInstall: false,
      downloadUrl: "https://example.test/app.deb",
      releaseUrl: RELEASES_PAGE,
    };

    await installUpdate(result);

    expect(invokeMock).toHaveBeenCalledWith("open_url", {
      url: "https://example.test/app.deb",
    });
  });

  it("falls back to the release URL when there is no download URL", async () => {
    const result: UpdateCheckResult = {
      status: "outdated",
      currentVersion: "1.2.0",
      canAutoInstall: false,
      releaseUrl: RELEASES_PAGE,
    };

    await installUpdate(result);

    expect(invokeMock).toHaveBeenCalledWith("open_url", { url: RELEASES_PAGE });
  });

  it("throws when neither a download URL nor a release URL is available", async () => {
    await expect(
      installUpdate({
        status: "error",
        currentVersion: "1.2.0",
        canAutoInstall: false,
        error: "no build",
      }),
    ).rejects.toThrow("no build");
  });
});

describe("updateStatusLabel", () => {
  it("has a non-empty label for every update status", () => {
    for (const label of Object.values(updateStatusLabel)) {
      expect(label.length).toBeGreaterThan(0);
    }
  });
});
