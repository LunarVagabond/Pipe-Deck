#!/usr/bin/env bash
# Single entry point tying a demo scenario to either a live dev run or a
# screenshot capture (issue #374) — the integration layer over scenario
# loading (#368) and screenshot tooling (#366/#367), not new capability of
# its own. Companion-app launching and window arrangement are deliberately
# not handled here yet: per the #372 spike, window positioning has no
# reliable scriptable surface on this project's actual recording
# environment (Pop!_OS/COSMIC/Wayland) and stays a manual pre-recording
# step; companion-app launching is a real follow-up once a scenario file
# has somewhere to declare which apps it wants running.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat <<EOF
Usage: make demo SCENARIO=<name> [MODE=dev|screenshot]

  SCENARIO   Name of a scenario file under scenarios/ (without .yaml), e.g. podcast
  MODE       "dev"        (default) — launch Pipe Deck's dev build (Tauri + Vite)
                           against the scenario, ready for a live recording.
             "screenshot" — capture the scenario's views into
                           docs/images/demo/<scenario>/ instead of launching the
                           app. Never overwrites the checked-in default
                           docs/images/*.png.

Examples:
  make demo SCENARIO=podcast
  make demo SCENARIO=streaming-multi-output MODE=screenshot

Available scenarios:
EOF
  for file in "$REPO_ROOT"/scenarios/*.yaml; do
    [ -e "$file" ] || continue
    printf '  - %s\n' "$(basename "$file" .yaml)"
  done
}

SCENARIO="${SCENARIO:-}"
MODE="${MODE:-dev}"

if [ -z "$SCENARIO" ]; then
  echo "error: SCENARIO is required" >&2
  usage >&2
  exit 1
fi

SCENARIO_FILE="$REPO_ROOT/scenarios/$SCENARIO.yaml"
if [ ! -f "$SCENARIO_FILE" ]; then
  echo "error: no scenario file at scenarios/$SCENARIO.yaml" >&2
  usage >&2
  exit 1
fi

export PIPE_DECK_MOCK_SCENARIO="$SCENARIO_FILE"

case "$MODE" in
  dev)
    echo "demo runner: launching Pipe Deck against scenario '$SCENARIO'"
    echo "demo runner: PIPE_DECK_MOCK_SCENARIO=$SCENARIO_FILE"
    exec make -C "$REPO_ROOT" dev-mock
    ;;
  screenshot)
    OUT_DIR="docs/images/demo/$SCENARIO"
    echo "demo runner: capturing screenshots for scenario '$SCENARIO' into $OUT_DIR/"
    export PIPE_DECK_SCREENSHOT_OUTPUT_DIR="$OUT_DIR"
    make -C "$REPO_ROOT" build-cli
    node "$REPO_ROOT/scripts/screenshot-app.mjs"
    ;;
  *)
    echo "error: unknown MODE '$MODE' (expected 'dev' or 'screenshot')" >&2
    exit 1
    ;;
esac
