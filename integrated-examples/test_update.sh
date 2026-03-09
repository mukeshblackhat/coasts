#!/usr/bin/env bash
#
# Integration test: update safety coordination.
#
# Verifies the daemon-side update coordination contract without performing
# a real self-update:
#   1. `GET /api/v1/update/is-safe-to-update` reports warnings for stale state
#   2. `POST /api/v1/update/prepare-for-update` enables quiescing and cleans up
#      a dangling managed container
#   3. New mutating commands are rejected while quiescing is active
#   4. `coast daemon restart` clears quiescing and brings the daemon back cleanly
#
# This test intentionally does NOT call the live GitHub release API or run
# `coast update apply`. It focuses on the daemon coordination layer.
#
# We set a temp COAST_HOME so ordinary CLI commands like `coast daemon status`
# and `coast daemon restart` skip the global auto-update policy. Otherwise the
# copied test binary could self-update to the latest GitHub release mid-test,
# which would defeat the purpose of exercising the local source build.
#
# Prerequisites:
#   - Docker running
#   - socat installed (brew install socat)
#   - curl installed
#   - Coast binaries built (cargo build --release)
#
# Usage:
#   ./integrated-examples/test_update.sh

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/helpers.sh"

ORIGINAL_HOME="${HOME:-}"
ORIGINAL_PATH="${PATH:-}"
ORIGINAL_COAST_HOME="${COAST_HOME:-}"
RELEASE_COAST="$COAST"
RELEASE_COASTD="$COASTD"

TEST_HOME=""
TEST_BIN_DIR=""
DANGLING_CONTAINER=""
TEST_API_PORT="31417"
TEST_DNS_PORT="5356"

cleanup() {
    echo ""
    echo "--- Cleaning up ---"

    if [ -n "${COAST:-}" ] && [ -x "${COAST:-}" ]; then
        "$COAST" daemon kill 2>/dev/null || true
    fi
    pkill -f "coastd --foreground" 2>/dev/null || true
    pkill -f "coastd" 2>/dev/null || true
    sleep 1

    if [ -n "$DANGLING_CONTAINER" ]; then
        docker rm -f "$DANGLING_CONTAINER" 2>/dev/null || true
    fi

    if [ -n "$TEST_HOME" ]; then
        rm -rf "$TEST_HOME" 2>/dev/null || true
    fi

    export HOME="$ORIGINAL_HOME"
    export PATH="$ORIGINAL_PATH"
    if [ -n "$ORIGINAL_COAST_HOME" ]; then
        export COAST_HOME="$ORIGINAL_COAST_HOME"
    else
        unset COAST_HOME 2>/dev/null || true
    fi

    echo "Cleanup complete."
}

trap cleanup EXIT

echo "=== test_update.sh — update coordination smoke test ==="
echo ""

TEST_HOME="$(mktemp -d)"
TEST_BIN_DIR="$TEST_HOME/bin"
mkdir -p "$TEST_BIN_DIR"

cp "$RELEASE_COAST" "$TEST_BIN_DIR/coast"
cp "$RELEASE_COASTD" "$TEST_BIN_DIR/coastd"
chmod +x "$TEST_BIN_DIR/coast" "$TEST_BIN_DIR/coastd"

export HOME="$TEST_HOME"
export PATH="$TEST_BIN_DIR:$ORIGINAL_PATH"
export COAST_HOME="$TEST_HOME/.coast"
export COAST_API_PORT="$TEST_API_PORT"
export COAST_DNS_PORT="$TEST_DNS_PORT"
COAST="$TEST_BIN_DIR/coast"
COASTD="$TEST_BIN_DIR/coastd"

preflight_checks
command -v curl >/dev/null || fail "curl not installed"
pass "curl installed"

clean_slate
"$COASTD" --foreground &>/tmp/coastd-test.log &
sleep 2
pass "Daemon started"

API_BASE="http://127.0.0.1:${TEST_API_PORT}/api/v1/update"

STATUS_OUT=$("$COAST" daemon status 2>&1 || true)
assert_contains "$STATUS_OUT" "is running" "daemon is running before update checks"

PID_BEFORE=$(cat "$HOME/.coast/coastd.pid" 2>/dev/null | tr -d '[:space:]')
[ -n "$PID_BEFORE" ] || fail "PID file should exist after daemon start"
pass "daemon started with PID $PID_BEFORE"

echo ""
echo "=== Seed dangling managed container ==="

docker pull alpine:3.20 >/dev/null 2>&1 || true
DANGLING_CONTAINER="coast-update-dangling-$$"
docker run -d \
    --name "$DANGLING_CONTAINER" \
    --label coast.managed=true \
    --label coast.project=update-test \
    --label coast.instance=dangler \
    alpine:3.20 sleep 300 >/dev/null
pass "dangling managed container created"

echo ""
echo "=== Test 1: is-safe-to-update warning report ==="

SAFE_OUT=$(curl -fsS "$API_BASE/is-safe-to-update")
assert_contains "$SAFE_OUT" '"safe":true' "is-safe-to-update reports safe when only warnings exist"
assert_contains "$SAFE_OUT" "dangling Coast-managed container" "is-safe-to-update reports dangling container warning"

echo ""
echo "=== Test 2: prepare-for-update quiesces and cleans stale state ==="

PREP_OUT=$(curl -fsS \
    -X POST \
    -H "Content-Type: application/json" \
    -d '{"timeout_ms":5000,"close_sessions":false,"stop_running_instances":false,"stop_shared_services":false}' \
    "$API_BASE/prepare-for-update")
assert_contains "$PREP_OUT" '"ready":true' "prepare-for-update reports ready"
assert_contains "$PREP_OUT" '"quiescing":true' "prepare-for-update leaves daemon quiescing"
assert_contains "$PREP_OUT" "Removed dangling managed container" "prepare-for-update reports dangling cleanup"

if docker inspect "$DANGLING_CONTAINER" >/dev/null 2>&1; then
    fail "dangling managed container should be removed during prepare-for-update"
fi
pass "dangling managed container removed during prepare-for-update"
DANGLING_CONTAINER=""

echo ""
echo "=== Test 3: quiescing rejects new mutating commands ==="

BLOCK_OUT=$("$COAST" --project update-test rm nonexistent 2>&1 || true)
assert_contains "$BLOCK_OUT" "preparing for an update" "mutating command rejected while quiescing"

SAFE_QUIESCED=$(curl -fsS "$API_BASE/is-safe-to-update")
assert_contains "$SAFE_QUIESCED" '"quiescing":true' "is-safe-to-update reports quiescing after prepare"

echo ""
echo "=== Test 4: daemon restart clears quiescing ==="

"$COAST" daemon restart 2>&1 || fail "coast daemon restart failed after prepare-for-update"
sleep 2

STATUS_AFTER=$("$COAST" daemon status 2>&1 || true)
assert_contains "$STATUS_AFTER" "is running" "daemon is running after restart"

PID_AFTER=$(cat "$HOME/.coast/coastd.pid" 2>/dev/null | tr -d '[:space:]')
[ -n "$PID_AFTER" ] || fail "PID file should exist after daemon restart"
if [ "$PID_BEFORE" = "$PID_AFTER" ]; then
    fail "daemon PID should change after restart (before=$PID_BEFORE, after=$PID_AFTER)"
fi
pass "daemon restarted with new PID $PID_AFTER"

POST_RESTART_OUT=$("$COAST" --project update-test rm nonexistent 2>&1 || true)
assert_not_contains "$POST_RESTART_OUT" "preparing for an update" "quiescing cleared after daemon restart"
assert_contains "$POST_RESTART_OUT" "not found" "mutating command reaches normal handler after restart"

SAFE_AFTER=$(curl -fsS "$API_BASE/is-safe-to-update")
assert_contains "$SAFE_AFTER" '"safe":true' "is-safe-to-update reports safe after daemon restart"
assert_contains "$SAFE_AFTER" '"quiescing":false' "quiescing disabled after daemon restart"

echo ""
echo "==========================================="
echo "  ALL UPDATE COORDINATION TESTS PASSED"
echo "==========================================="
