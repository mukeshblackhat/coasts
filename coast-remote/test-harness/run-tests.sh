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

# curl wrapper that shows errors instead of swallowing them
api_get() {
    local resp
    resp=$(curl -s --max-time 10 "$REMOTE/api/v1$1" 2>&1)
    local code=$?
    if [ $code -ne 0 ]; then
        echo "CURL_ERROR: exit=$code resp=$resp" >&2
        echo ""
        return 1
    fi
    echo "$resp"
}

api_post() {
    local path="$1"
    local body="$2"
    local resp
    resp=$(curl -s --max-time 30 -X POST "$REMOTE/api/v1$path" \
        -H "Content-Type: application/json" \
        -d "$body" 2>&1)
    local code=$?
    if [ $code -ne 0 ]; then
        echo "CURL_ERROR: exit=$code resp=$resp" >&2
        echo ""
        return 1
    fi
    echo "$resp"
}

wait_for_service() {
    local url="$1"
    local name="$2"
    local max_wait=60

    echo "⏳ Waiting for $name..."
    for i in $(seq 1 $max_wait); do
        if curl -sf --max-time 3 "$url" >/dev/null 2>&1; then
            echo "  $name is ready (${i}s)"
            return 0
        fi
        sleep 1
    done
    echo "  ERROR: $name not ready after ${max_wait}s"
    exit 1
}

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

HEALTH=$(api_get "/health")
STATUS=$(echo "$HEALTH" | jq -r '.status' 2>/dev/null)
DOCKER=$(echo "$HEALTH" | jq -r '.docker' 2>/dev/null)

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

MOUNT_RESP=$(api_post "/mount" '{
    "project": "testapp",
    "ssh_target": "testuser@local-machine",
    "remote_path": "/home/testuser/project"
}')

MOUNT_STATUS=$(echo "$MOUNT_RESP" | jq -r '.status' 2>/dev/null)
MOUNT_PATH=$(echo "$MOUNT_RESP" | jq -r '.mount_path' 2>/dev/null)

if [ "$MOUNT_STATUS" = "mounted" ]; then
    pass "SSHFS mount succeeded (path: $MOUNT_PATH)"
else
    fail "SSHFS mount" "got: $MOUNT_RESP"
fi

MOUNTS=$(api_get "/mounts")
MOUNT_COUNT=$(echo "$MOUNTS" | jq '.mounts | length' 2>/dev/null)

if [ "$MOUNT_COUNT" -ge 1 ] 2>/dev/null; then
    pass "mount appears in /mounts list"
else
    fail "mount listing" "got: $MOUNTS"
fi

###############################################################################
# Test 3: Run a container
###############################################################################

echo ""
echo "── Test 3: Run container ──"

RUN_RESP=$(api_post "/container/run" '{
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

CONTAINER_ID=$(echo "$RUN_RESP" | jq -r '.container_id' 2>/dev/null)

if [ -n "$CONTAINER_ID" ] && [ "$CONTAINER_ID" != "null" ]; then
    pass "container created (id: ${CONTAINER_ID:0:12}...)"
else
    fail "container creation" "got: $RUN_RESP"
fi

# Wait for container to fully start
echo "  waiting 5s for container to settle..."
sleep 5

# Verify coast-remote is still reachable after container creation
echo "  checking coast-remote is still alive..."
HEALTH2=$(api_get "/health")
H2_STATUS=$(echo "$HEALTH2" | jq -r '.status' 2>/dev/null)
if [ "$H2_STATUS" = "ok" ]; then
    echo "  coast-remote still healthy"
else
    echo "  WARNING: coast-remote may be down: $HEALTH2"
fi

###############################################################################
# Test 4: Status shows container
###############################################################################

echo ""
echo "── Test 4: Status endpoint ──"

STATUS_RESP=$(api_get "/status")
echo "  raw response: $STATUS_RESP"
CONTAINER_COUNT=$(echo "$STATUS_RESP" | jq '.containers | length' 2>/dev/null)

if [ "$CONTAINER_COUNT" -ge 1 ] 2>/dev/null; then
    pass "status shows $CONTAINER_COUNT container(s)"
else
    fail "status" "count=$CONTAINER_COUNT got: $STATUS_RESP"
fi

###############################################################################
# Test 5: Exec — verify project files are mounted via SSHFS
###############################################################################

echo ""
echo "── Test 5: Exec (verify SSHFS files in container) ──"

EXEC_RESP=$(api_post "/container/exec" \
    '{"project":"testapp","instance":"dev-1","cmd":["ls","/host-project"]}')

echo "  raw exec response: $EXEC_RESP"
EXEC_STDOUT=$(echo "$EXEC_RESP" | jq -r '.stdout' 2>/dev/null)
EXEC_CODE=$(echo "$EXEC_RESP" | jq -r '.exit_code' 2>/dev/null)

if [ "$EXEC_CODE" = "0" ] && echo "$EXEC_STDOUT" | grep -q "package.json"; then
    pass "project files visible inside container (found package.json)"
else
    fail "exec ls /host-project" "exit_code=$EXEC_CODE stdout=$EXEC_STDOUT"
fi

EXEC_RESP2=$(api_post "/container/exec" \
    '{"project":"testapp","instance":"dev-1","cmd":["cat","/host-project/index.js"]}')

EXEC_STDOUT2=$(echo "$EXEC_RESP2" | jq -r '.stdout' 2>/dev/null)

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

IP_RESP=$(api_post "/container/ip" \
    '{"project":"testapp","instance":"dev-1"}')

IP=$(echo "$IP_RESP" | jq -r '.ip' 2>/dev/null)

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

STOP_RESP=$(api_post "/container/stop" \
    '{"project":"testapp","instance":"dev-1"}')

STOP_STATUS=$(echo "$STOP_RESP" | jq -r '.status' 2>/dev/null)

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

RM_RESP=$(api_post "/container/rm" \
    '{"project":"testapp","instance":"dev-1"}')

RM_STATUS=$(echo "$RM_RESP" | jq -r '.status' 2>/dev/null)

if [ "$RM_STATUS" = "removed" ]; then
    pass "container removed"
else
    fail "remove" "got: $RM_RESP"
fi

STATUS_AFTER=$(api_get "/status")
CONTAINERS_AFTER=$(echo "$STATUS_AFTER" | jq '.containers | length' 2>/dev/null)

if [ "$CONTAINERS_AFTER" = "0" ] 2>/dev/null; then
    pass "status shows 0 containers after removal"
else
    fail "post-removal status" "still shows $CONTAINERS_AFTER containers"
fi

###############################################################################
# Test 9: Unmount
###############################################################################

echo ""
echo "── Test 9: Unmount SSHFS ──"

UNMOUNT_RESP=$(api_post "/unmount" '{"project":"testapp"}')

UNMOUNT_STATUS=$(echo "$UNMOUNT_RESP" | jq -r '.status' 2>/dev/null)

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
