#!/usr/bin/env node
// Refreshes docs/images/*.png from the live frontend, so README/docs screenshots
// stay in sync with the UI instead of going stale across releases.
//
// Runs the frontend alone (`vite`, no Tauri shell) and injects a
// window.__TAURI_INTERNALS__ shim that answers the handful of commands each
// captured view needs on mount. The sample graph itself comes from
// `pipe-deck-cli graph` run against PIPE_DECK_USE_MOCK=1 — i.e. the exact same
// `MockAudioBackend::sample_graph()` (`src-tauri/src/backend/mock.rs`) the
// real app seeds, not a second hand-copied JS object that could drift from it
// (issue #366). PIPE_DECK_USE_MOCK only affects the Rust backend — a bare
// `vite` dev server has no Tauri IPC at all — so this script still has to
// shim window.__TAURI_INTERNALS__ itself; it just sources the data for that
// shim from the real backend instead of reinventing it.
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

const repoRoot = dirname(dirname(fileURLToPath(import.meta.url)));
const imagesDir = join(repoRoot, "docs", "images");
const port = 4317;
const baseUrl = `http://localhost:${port}`;

function loadMockRuntimeGraph() {
  const cargoTargetDir = process.env.CARGO_TARGET_DIR ?? join(repoRoot, "src-tauri", "target");
  const cliBin = join(cargoTargetDir, "debug", "pipe-deck-cli");
  if (!existsSync(cliBin)) {
    throw new Error(`pipe-deck-cli binary not found at ${cliBin} — run \`make build-cli\` first`);
  }

  // Isolated config dir so this never touches (or creates) the real
  // ~/.config/pipe-deck/ on the machine running the screenshot script.
  const scratchConfigDir = mkdtempSync(join(tmpdir(), "pipe-deck-screenshot-config-"));
  try {
    const result = spawnSync(cliBin, ["graph"], {
      env: { ...process.env, PIPE_DECK_USE_MOCK: "1", PIPE_DECK_CONFIG_DIR: scratchConfigDir },
      encoding: "utf8",
    });
    if (result.status !== 0) {
      throw new Error(`pipe-deck-cli graph failed: ${result.stderr || result.stdout}`);
    }
    const graph = JSON.parse(result.stdout);
    // Deliberately drop data_source/notice — every view gates its "Showing
    // sample data" banner on data_source === "mock", and these captures are
    // meant to read as ordinary screenshots of the real UI, not a labeled demo.
    delete graph.data_source;
    delete graph.notice;
    return graph;
  } finally {
    rmSync(scratchConfigDir, { recursive: true, force: true });
  }
}

const runtimeGraph = loadMockRuntimeGraph();

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
