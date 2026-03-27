#!/bin/bash
set -e

cd "$(dirname "$0")"

echo "Coast Remote POC — Test Harness"
echo "================================"
echo ""
echo "This will:"
echo "  1. Build coast-remote from source"
echo "  2. Spin up 'local' (SSH server + project files)"
echo "  3. Spin up 'remote' (coast-remote + Docker daemon)"
echo "  4. Run automated tests (mount, run container, exec, verify files, cleanup)"
echo ""

# Clean up on exit
cleanup() {
    echo ""
    echo "Cleaning up..."
    docker compose down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

# Build and run
docker compose up --build --abort-on-container-exit --exit-code-from test
