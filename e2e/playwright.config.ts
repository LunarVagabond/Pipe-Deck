import { defineConfig, devices } from "@playwright/test";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const port = 4173;

/**
 * These are component-level UI tests, not full end-to-end app tests: Pipe Deck
 * is a Tauri desktop app (Vue frontend shelling out to pactl/pw-link), and this
 * suite doesn't drive the compiled Tauri binary or a real PipeWire graph.
 * Instead it serves the Vite frontend directly and mounts individual
 * components (e.g. RoutingGraph.vue) against a synthetic RuntimeGraph fixture
 * — see fixtures/. That's enough to catch regressions in pure frontend
 * rendering/reactivity logic (Vue Flow node/edge sync, layout, etc.) without
 * needing a live backend.
 */
export default defineConfig({
  testDir: ".",
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  reporter: "list",
  use: {
    baseURL: `http://localhost:${port}`,
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    command: `npx vite --port ${port} --strictPort`,
    cwd: repoRoot,
    port,
    reuseExistingServer: !process.env.CI,
  },
});
