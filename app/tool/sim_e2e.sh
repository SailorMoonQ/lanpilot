#!/usr/bin/env bash
# Simulator integration test (spec 7): starts a real agent on this Mac with
# mock input, runs integration_test/e2e_test.dart on a booted iOS simulator,
# then checks the agent saw pointer motion, a left click and Play/Pause.
#
# Usage: app/tool/sim_e2e.sh [simulator-udid]   (default: the booted one)
set -euo pipefail

APP_DIR="$(cd "$(dirname "$0")/.." && pwd)"
REPO_DIR="$(cd "$APP_DIR/.." && pwd)"
WORK="$(mktemp -d)"
LOG="$WORK/agent.log"

cleanup() {
  if [[ -n "${AGENT_PID:-}" ]]; then kill "$AGENT_PID" 2>/dev/null || true; fi
}
trap cleanup EXIT

DEVICE="${1:-}"
if [[ -z "$DEVICE" ]]; then
  DEVICE="$(xcrun simctl list devices booted | grep -Eo '[0-9A-F-]{36}' | head -1 || true)"
fi
if [[ -z "$DEVICE" ]]; then
  echo "No booted simulator. Boot one (xcrun simctl boot <udid>) or pass its udid." >&2
  exit 1
fi

(cd "$REPO_DIR" && cargo build -q -p lanpilot-agent)

# On the simulator NWBrowser sometimes never resolves a freshly started agent
# (the app's resolver stays "preparing"); a fresh agent and a new run clear it.
# Only the nearby check can fail this way, so retry the whole run a few times.
run_once() {
  RUST_LOG=info,lanpilot_input=debug \
    "$REPO_DIR/target/debug/lanpilot-agent" --home "$WORK/home-$attempt" run --pair --mock-input \
    >"$LOG" 2>&1 &
  AGENT_PID=$!

  for _ in $(seq 1 150); do
    grep -q 'lanpilot://pair?d=' "$LOG" && break
    sleep 0.2
  done
  URI="$(grep -Eo 'lanpilot://pair\?d=[A-Za-z0-9_-]+' "$LOG" | head -1 || true)"
  ID="$(sed -n 's/.*(\([0-9a-f]\{16\}\)) listening on UDP.*/\1/p' "$LOG" | head -1)"
  if [[ -z "$URI" ]]; then
    echo "The agent printed no pairing link:" >&2
    cat "$LOG" >&2
    exit 1
  fi

  cd "$APP_DIR"
  flutter test integration_test/e2e_test.dart -d "$DEVICE" \
    --dart-define=PAIR_URI="$URI" --dart-define=AGENT_ID="$ID"
}

for attempt in 1 2 3; do
  if run_once; then break; fi
  if [[ "$attempt" == 3 ]]; then
    echo "The integration test failed 3 times." >&2
    exit 1
  fi
  echo "Integration test failed (attempt $attempt), retrying with a fresh agent." >&2
  cleanup
  AGENT_PID=
done

sleep 1
for pattern in 'mock input: move' 'mock input: Left button down' 'mock input: media PlayPause'; do
  if ! grep -q "$pattern" "$LOG"; then
    echo "The agent log lacks \"$pattern\":" >&2
    cat "$LOG" >&2
    exit 1
  fi
done
echo "Simulator e2e passed (agent log: $LOG)"
