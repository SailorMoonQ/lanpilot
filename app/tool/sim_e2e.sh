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

# SIGINT (not SIGTERM) so the agent withdraws its mDNS record; stale records
# would pile up on this Mac and in every browser on the network.
cleanup() {
  if [[ -z "${AGENT_PID:-}" ]]; then return; fi
  kill -INT "$AGENT_PID" 2>/dev/null || true
  for _ in $(seq 1 50); do
    kill -0 "$AGENT_PID" 2>/dev/null || break
    sleep 0.1
  done
  kill -KILL "$AGENT_PID" 2>/dev/null || true
  wait "$AGENT_PID" 2>/dev/null || true
  AGENT_PID=
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

RUST_LOG=info,lanpilot_input=debug \
  "$REPO_DIR/target/debug/lanpilot-agent" --home "$WORK/home" run --pair --mock-input \
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

sleep 1
for pattern in 'mock input: move' 'mock input: Left button down' 'mock input: media PlayPause'; do
  if ! grep -q "$pattern" "$LOG"; then
    echo "The agent log lacks \"$pattern\":" >&2
    cat "$LOG" >&2
    exit 1
  fi
done
echo "Simulator e2e passed (agent log: $LOG)"
