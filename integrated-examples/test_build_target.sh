#!/usr/bin/env bash
#
# Integration test for build.target support in compose files.
#
# Verifies that coast build honors the `target` field in a compose
# service's `build:` directive by building only the specified stage
# of a multi-stage Dockerfile.
#
# The test project has a multi-stage Dockerfile:
#   - "dev" stage: no /app/stage.txt, /stage endpoint returns "development"
#   - "prod" stage: creates /app/stage.txt, /stage endpoint returns "production"
#
# docker-compose.yml sets `target: dev`, so the running container
# should report "development".
#
# Prerequisites:
#   - Docker running
#   - socat installed (brew install socat)
#   - Coast binaries built (cargo build --release)
#
# Usage:
#   ./integrated-examples/test_build_target.sh

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/helpers.sh"

register_cleanup

# --- Preflight ---

preflight_checks

# --- Setup ---

echo ""
echo "=== Setup ==="

clean_slate

"$HELPERS_DIR/setup.sh"
pass "Examples initialized"

cd "$PROJECTS_DIR/coast-build-target"

start_daemon

# ============================================================
# Test 1: Build with target
# ============================================================

echo ""
echo "=== Test 1: coast build (with build.target) ==="

BUILD_OUT=$("$COAST" build 2>&1)
assert_contains "$BUILD_OUT" "Build complete" "coast build succeeds"
pass "Build complete"

# ============================================================
# Test 2: Run and verify the correct stage was built
# ============================================================

echo ""
echo "=== Test 2: coast run + verify stage ==="

RUN_OUT=$("$COAST" run inst-a 2>&1)
CLEANUP_INSTANCES+=("inst-a")
assert_contains "$RUN_OUT" "Created coast instance" "coast run inst-a succeeds"

DYN_PORT=$(extract_dynamic_port "$RUN_OUT" "app")
[ -n "$DYN_PORT" ] || fail "Could not extract app dynamic port"
pass "dynamic port: $DYN_PORT"

wait_for_healthy "$DYN_PORT" 60 || fail "app did not become healthy"
pass "app is healthy"

STAGE_RESP=$(curl -s "http://localhost:${DYN_PORT}/stage")
assert_contains "$STAGE_RESP" '"stage":"development"' "build.target=dev was honored (got development stage)"

# ============================================================
# Test 3: Cleanup
# ============================================================

echo ""
echo "=== Test 3: cleanup ==="

"$COAST" rm inst-a 2>&1 | grep -q "Removed" || fail "coast rm inst-a failed"
pass "coast rm inst-a succeeded"
CLEANUP_INSTANCES=()

# --- Done ---

echo ""
echo "==========================================="
echo "  ALL BUILD TARGET TESTS PASSED"
echo "==========================================="
