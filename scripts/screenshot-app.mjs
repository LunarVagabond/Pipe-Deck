#!/usr/bin/env node
// Refreshes docs/images/*.png from the live frontend, so README/docs screenshots
// stay in sync with the UI instead of going stale across releases.
//
// Runs the frontend alone (`vite`, no Tauri shell) and injects a
// window.__TAURI_INTERNALS__ shim that answers the handful of commands each
// captured view needs on mount, mirroring the same sample graph
// `src-tauri/src/backend/mock.rs` seeds for PIPE_DECK_USE_MOCK=1 (that env var
// only affects the Rust backend — a bare `vite` dev server has no Tauri IPC at
// all, so PIPE_DECK_USE_MOCK alone doesn't get you real-looking data here).
import { spawn } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

const repoRoot = dirname(dirname(fileURLToPath(import.meta.url)));
const imagesDir = join(repoRoot, "docs", "images");
const port = 4317;
const baseUrl = `http://localhost:${port}`;

const runtimeGraph = {
  devices: [
    device("sink-chat", "Chat", "virtual", "output", { current_target: "sink-headphones", current_targets: ["sink-headphones"] }),
    device("sink-music", "Music", "virtual", "output", { current_target: "sink-headphones", current_targets: ["sink-headphones", "sink-stream-output"] }),
    device("sink-game", "Game", "virtual", "output", { current_target: "sink-headphones", current_targets: ["sink-headphones"] }),
    device("sink-browser", "Browser", "virtual", "output", { current_target: "sink-speakers", current_targets: ["sink-speakers"] }),
    device("sink-stream-mix", "Stream Mix", "virtual", "output", { current_target: "sink-stream-output", current_targets: ["sink-stream-output"] }),
    device("sink-headphones", "Headphones", "physical", "output"),
    device("sink-speakers", "Speakers", "physical", "output"),
    device("sink-stream-output", "Stream Output", "virtual", "output"),
    device("source-mic", "Microphone", "physical", "input"),
    device("source-mic-filtered", "Mic (Filtered)", "virtual", "input", {
      mix_sources: [{ device_id: "source-mic", volume_percent: 100, muted: false }],
    }),
  ],
  streams: [
    stream("stream-discord", "Discord", "discord", "playback", "sink-chat"),
    stream("stream-spotify", "Spotify", "spotify", "playback", "sink-music"),
    stream("stream-steam", "Steam", "steam", "playback", "sink-game"),
    stream("stream-firefox", "Firefox", "firefox", "playback", "sink-browser"),
    stream("stream-obs", "OBS", "obs", "capture", "source-mic-filtered"),
  ],
  links: [
    link("link-discord-chat", "stream-discord", "sink-chat"),
    link("link-spotify-music", "stream-spotify", "sink-music"),
    link("link-steam-game", "stream-steam", "sink-game"),
    link("link-firefox-browser", "stream-firefox", "sink-browser"),
    link("link-chat-headphones", "sink-chat", "sink-headphones"),
    link("link-music-headphones", "sink-music", "sink-headphones"),
    link("link-music-stream", "sink-music", "sink-stream-output"),
    link("link-game-headphones", "sink-game", "sink-headphones"),
    link("link-browser-speakers", "sink-browser", "sink-speakers"),
    link("link-stream-mix-output", "sink-stream-mix", "sink-stream-output"),
    link("link-obs-mic", "source-mic-filtered", "stream-obs"),
    link("link-mic-filtered", "source-mic", "source-mic-filtered"),
  ],
  // Deliberately omit data_source: "mock" — every view gates its
  // "Showing sample data" banner on that field, and these captures are meant
  // to read as ordinary screenshots of the real UI, not as a labeled demo.
};

function device(id, label, kind, direction, extra = {}) {
  return {
    id,
    system_name: id,
    label,
    kind,
    direction,
    volume_percent: 70,
    muted: false,
    current_targets: [],
    mix_sources: [],
    ...extra,
  };
}

function stream(id, appName, executable, direction, target) {
  return {
    id,
    app_name: appName,
    executable,
    system_name: id,
    direction,
    current_target: target,
    is_system: false,
  };
}

function link(id, sourceId, targetId) {
  return { id, source_id: sourceId, target_id: targetId };
}

const appConfig = { version: 1, profile_index: [], preferences: { theme_mode: "dark" } };
const daemonStatus = { running: true, enabled: true, devices_restored: 7 };
const appInfo = {
  buildRevision: "0.0.5",
  installKind: "dev",
  backgroundRestoreSupported: false,
  installLabel: "Dev build",
};
const themeColors = {
  background: "#0b0d12",
  surface_1: "#12151c",
  surface_2: "#181c26",
  border: "#262b36",
  text: "#f4f6fb",
  text_muted: "#9aa3b2",
  accent_purple: "#8b7bff",
  accent_teal: "#3ddbd9",
  accent_amber: "#f5b95c",
  status_success: "#3ddc84",
  status_warning: "#f5b95c",
  status_danger: "#ef5b6b",
};
const themes = [{ id: "midnight-deck", name: "Midnight Deck", kind: "dark", source: "builtin", colors: themeColors }];

const commandResponses = {
  get_runtime_graph: runtimeGraph,
  get_config: appConfig,
  get_config_paths: { config_dir: "", config_file: "" },
  list_profiles: [],
  list_themes: themes,
  get_daemon_status: daemonStatus,
  get_app_info: appInfo,
};

// Everything else the boot sequence/views call on mount or on user
// interaction (sidebar/theme/stream-visibility toggles, etc.) is a
// fire-and-forget mutation against a backend that doesn't exist here — a
// resolved no-op is enough since the frontend already treats these
// optimistically and only reconciles on the next graph fetch.
const noopCommands = new Set([
  "set_sidebar_collapsed",
  "set_show_system_streams",
  "set_auto_apply_rules",
  "set_theme_mode",
  "set_dark_scheme",
  "set_light_scheme",
  "plugin:event|listen",
  "plugin:event|unlisten",
]);

const views = [
  { id: "dashboard", label: "Dashboard", file: "dashboard.png" },
  { id: "mixer", label: "Mixer", file: "mixer.png" },
  { id: "routing", label: "Routing", file: "routing.png" },
  { id: "sources", label: "Sources", file: "sources.png" },
];

function waitForServer(url, timeoutMs = 30000) {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const attempt = () => {
      fetch(url)
        .then(() => resolve())
        .catch((err) => {
          if (Date.now() > deadline) {
            reject(err);
            return;
          }
          setTimeout(attempt, 300);
        });
    };
    attempt();
  });
}

async function main() {
  if (!existsSync(imagesDir)) mkdirSync(imagesDir, { recursive: true });

  const viteBin = join(repoRoot, "node_modules", ".bin", "vite");
  const vite = spawn(viteBin, ["--port", String(port), "--strictPort"], {
    cwd: repoRoot,
    stdio: "inherit",
  });

  const cleanup = () => {
    vite.kill();
  };
  process.on("exit", cleanup);

  try {
    await waitForServer(baseUrl);

    const browser = await chromium.launch();
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });

    await page.addInitScript((responses) => {
      window.__TAURI_INTERNALS__ = {
        invoke: (cmd) => {
          if (Object.prototype.hasOwnProperty.call(responses.data, cmd)) {
            return Promise.resolve(responses.data[cmd]);
          }
          if (responses.noop.includes(cmd)) {
            return Promise.resolve(cmd === "plugin:event|listen" ? 1 : null);
          }
          return Promise.resolve(null);
        },
        transformCallback: () => 0,
        unregisterCallback: () => {},
      };
    }, { data: commandResponses, noop: [...noopCommands] });

    await page.goto(baseUrl, { waitUntil: "networkidle" });
    await page.waitForSelector(".nav-item");

    for (const view of views) {
      await page.getByRole("link", { name: view.label, exact: true }).click();
      await page.waitForTimeout(400);
      await page.screenshot({ path: join(imagesDir, view.file), fullPage: true });
      console.log(`Captured ${view.file}`);
    }

    await browser.close();
  } finally {
    cleanup();
  }
}

main().catch((err) => {
  console.error(err);
  process.exitCode = 1;
});
