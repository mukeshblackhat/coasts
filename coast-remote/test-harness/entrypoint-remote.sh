#!/bin/bash
set -e

echo "==> Starting Docker daemon..."
dockerd-entrypoint.sh &

# Wait for Docker to be ready
echo "==> Waiting for Docker daemon..."
for i in $(seq 1 30); do
    if docker info >/dev/null 2>&1; then
        echo "==> Docker is ready"
        break
    fi
    sleep 1
done

if ! docker info >/dev/null 2>&1; then
    echo "==> ERROR: Docker failed to start"
    exit 1
fi

# Setup SSH key for connecting to the "local" container
echo "==> Setting up SSH keys..."
mkdir -p /root/.ssh
if [ -f /ssh-keys/id_ed25519 ]; then
    cp /ssh-keys/id_ed25519 /root/.ssh/id_ed25519
    chmod 600 /root/.ssh/id_ed25519
    # Trust all hosts (test environment only)
    echo "StrictHostKeyChecking no" > /root/.ssh/config
    echo "UserKnownHostsFile /dev/null" >> /root/.ssh/config
    echo "LogLevel ERROR" >> /root/.ssh/config
fi

# Pre-pull images that tests will use
echo "==> Pre-pulling alpine image for tests..."
docker pull alpine:latest

echo "==> Starting coast-remote..."
exec coast-remote --port 31416 --mount-dir /mnt/coast
