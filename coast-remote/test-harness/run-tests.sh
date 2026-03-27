#!/bin/bash
# Don't set -e: we want to run all tests even if some fail

REMOTE="http://remote-machine:31416"
PASS=0
FAIL=0

apt-get update >/dev/null 2>&1 && apt-get install -y --no-install-recommends curl jq >/dev/null 2>&1

###############################################################################
# Helpers
###############################################################################

pass() {
    PASS=$((PASS + 1))
    echo "  ✓ $1"
}

fail() {
    FAIL=$((FAIL + 1))
    echo "  ✗ $1"
    echo "    → $2"
}

wait_for_service() {
    local url="$1"
    local name="$2"
    local max_wait=60

    echo "⏳ Waiting for $name..."
    for i in $(seq 1 $max_wait); do
        if curl -sf "$url" >/dev/null 2>&1; then
            echo "  $name is ready (${i}s)"
            return 0
        fi
        sleep 1
    done
    echo "  ERROR: $name not ready after ${max_wait}s"
    exit 1
}

###############################################################################
# Wait for services
###############################################################################

echo ""
echo "════════════════════════════════════════════════════════"
echo "  Coast Remote POC — Test Suite"
echo "════════════════════════════════════════════════════════"
echo ""

wait_for_service "$REMOTE/api/v1/health" "coast-remote"

###############################################################################
# Test 1: Health check
###############################################################################

echo ""
echo "── Test 1: Health check ──"

HEALTH=$(curl -sf "$REMOTE/api/v1/health")
STATUS=$(echo "$HEALTH" | jq -r '.status')
DOCKER=$(echo "$HEALTH" | jq -r '.docker')

if [ "$STATUS" = "ok" ] && [ "$DOCKER" = "connected" ]; then
    pass "health endpoint returns ok + docker connected"
else
    fail "health check" "got: $HEALTH"
fi

###############################################################################
# Test 2: Mount project via SSHFS
###############################################################################

echo ""
echo "── Test 2: SSHFS mount ──"

MOUNT_RESP=$(curl -sf -X POST "$REMOTE/api/v1/mount" \
    -H "Content-Type: application/json" \
    -d '{
        "project": "testapp",
        "ssh_target": "testuser@local-machine",
        "remote_path": "/home/testuser/project"
    }')

MOUNT_STATUS=$(echo "$MOUNT_RESP" | jq -r '.status')
MOUNT_PATH=$(echo "$MOUNT_RESP" | jq -r '.mount_path')

if [ "$MOUNT_STATUS" = "mounted" ]; then
    pass "SSHFS mount succeeded (path: $MOUNT_PATH)"
else
    fail "SSHFS mount" "got: $MOUNT_RESP"
fi

# Verify mount shows up in the list
MOUNTS=$(curl -sf "$REMOTE/api/v1/mounts")
MOUNT_COUNT=$(echo "$MOUNTS" | jq '.mounts | length')

if [ "$MOUNT_COUNT" -ge 1 ]; then
    pass "mount appears in /mounts list"
else
    fail "mount listing" "got: $MOUNTS"
fi

###############################################################################
# Test 3: Run a container (using alpine — fast to pull)
###############################################################################

echo ""
echo "── Test 3: Run container ──"

# Use alpine:latest with a long-running command so the container stays alive
RUN_RESP=$(curl -sf -X POST "$REMOTE/api/v1/container/run" \
    -H "Content-Type: application/json" \
    -d '{
        "config": {
            "project": "testapp",
            "instance_name": "dev-1",
            "image": "alpine:latest",
            "env_vars": {},
            "bind_mounts": [],
            "volume_mounts": [],
            "tmpfs_mounts": [],
            "networks": [],
            "working_dir": null,
            "entrypoint": null,
            "cmd": ["sleep", "300"],
            "labels": {},
            "published_ports": [],
            "extra_hosts": []
        }
    }')

CONTAINER_ID=$(echo "$RUN_RESP" | jq -r '.container_id')

if [ -n "$CONTAINER_ID" ] && [ "$CONTAINER_ID" != "null" ]; then
    pass "container created (id: ${CONTAINER_ID:0:12}...)"
else
    fail "container creation" "got: $RUN_RESP"
fi

# Wait for container to fully start
sleep 5

###############################################################################
# Test 4: Status shows container
###############################################################################

echo ""
echo "── Test 4: Status endpoint ──"

STATUS_RESP=$(curl -sf "$REMOTE/api/v1/status")
CONTAINER_COUNT=$(echo "$STATUS_RESP" | jq '.containers | length')

if [ "$CONTAINER_COUNT" -ge 1 ]; then
    pass "status shows $CONTAINER_COUNT container(s)"
else
    fail "status" "got: $STATUS_RESP"
fi

###############################################################################
# Test 5: Exec — verify project files are mounted via SSHFS
###############################################################################

echo ""
echo "── Test 5: Exec (verify SSHFS files in container) ──"

EXEC_RESP=$(curl -sf -X POST "$REMOTE/api/v1/container/exec" \
    -H "Content-Type: application/json" \
    -d '{"project":"testapp","instance":"dev-1","cmd":["ls","/host-project"]}')

EXEC_STDOUT=$(echo "$EXEC_RESP" | jq -r '.stdout')
EXEC_CODE=$(echo "$EXEC_RESP" | jq -r '.exit_code')

if [ "$EXEC_CODE" = "0" ] && echo "$EXEC_STDOUT" | grep -q "package.json"; then
    pass "project files visible inside container (found package.json)"
else
    fail "exec ls /host-project" "exit_code=$EXEC_CODE stdout=$EXEC_STDOUT"
fi

# Check specific file content
EXEC_RESP2=$(curl -sf -X POST "$REMOTE/api/v1/container/exec" \
    -H "Content-Type: application/json" \
    -d '{"project":"testapp","instance":"dev-1","cmd":["cat","/host-project/index.js"]}')

EXEC_STDOUT2=$(echo "$EXEC_RESP2" | jq -r '.stdout')

if echo "$EXEC_STDOUT2" | grep -q "hello from coast remote POC"; then
    pass "file content matches (index.js)"
else
    fail "exec cat index.js" "got: $EXEC_STDOUT2"
fi

###############################################################################
# Test 6: Get container IP
###############################################################################

echo ""
echo "── Test 6: Container IP ──"

IP_RESP=$(curl -sf -X POST "$REMOTE/api/v1/container/ip" \
    -H "Content-Type: application/json" \
    -d '{"project":"testapp","instance":"dev-1"}')

IP=$(echo "$IP_RESP" | jq -r '.ip')

if echo "$IP" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$'; then
    pass "got container IP: $IP"
else
    fail "container IP" "got: $IP_RESP"
fi

###############################################################################
# Test 7: Stop container
###############################################################################

echo ""
echo "── Test 7: Stop container ──"

STOP_RESP=$(curl -sf -X POST "$REMOTE/api/v1/container/stop" \
    -H "Content-Type: application/json" \
    -d '{"project":"testapp","instance":"dev-1"}')

STOP_STATUS=$(echo "$STOP_RESP" | jq -r '.status')

if [ "$STOP_STATUS" = "stopped" ]; then
    pass "container stopped"
else
    fail "stop" "got: $STOP_RESP"
fi

###############################################################################
# Test 8: Remove container
###############################################################################

echo ""
echo "── Test 8: Remove container ──"

RM_RESP=$(curl -sf -X POST "$REMOTE/api/v1/container/rm" \
    -H "Content-Type: application/json" \
    -d '{"project":"testapp","instance":"dev-1"}')

RM_STATUS=$(echo "$RM_RESP" | jq -r '.status')

if [ "$RM_STATUS" = "removed" ]; then
    pass "container removed"
else
    fail "remove" "got: $RM_RESP"
fi

# Verify it's gone from status
STATUS_AFTER=$(curl -sf "$REMOTE/api/v1/status")
CONTAINERS_AFTER=$(echo "$STATUS_AFTER" | jq '.containers | length')

if [ "$CONTAINERS_AFTER" = "0" ]; then
    pass "status shows 0 containers after removal"
else
    fail "post-removal status" "still shows $CONTAINERS_AFTER containers"
fi

###############################################################################
# Test 9: Unmount
###############################################################################

echo ""
echo "── Test 9: Unmount SSHFS ──"

UNMOUNT_RESP=$(curl -sf -X POST "$REMOTE/api/v1/unmount" \
    -H "Content-Type: application/json" \
    -d '{"project":"testapp"}')

UNMOUNT_STATUS=$(echo "$UNMOUNT_RESP" | jq -r '.status')

if [ "$UNMOUNT_STATUS" = "unmounted" ]; then
    pass "SSHFS unmounted"
else
    fail "unmount" "got: $UNMOUNT_RESP"
fi

###############################################################################
# Results
###############################################################################

TOTAL=$((PASS + FAIL))

echo ""
echo "════════════════════════════════════════════════════════"
echo "  Results: $PASS/$TOTAL passed"
if [ "$FAIL" -gt 0 ]; then
    echo "  $FAIL FAILED"
    echo "════════════════════════════════════════════════════════"
    exit 1
else
    echo "  All tests passed!"
    echo "════════════════════════════════════════════════════════"
    exit 0
fi
