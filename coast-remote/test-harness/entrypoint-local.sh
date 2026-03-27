#!/bin/bash
set -e

# Wait for the SSH key to be generated (by the ssh-keygen service)
echo "==> Waiting for SSH key..."
for i in $(seq 1 30); do
    if [ -f /ssh-keys/id_ed25519.pub ]; then
        break
    fi
    sleep 1
done

if [ ! -f /ssh-keys/id_ed25519.pub ]; then
    echo "==> ERROR: SSH key not found after 30s"
    exit 1
fi

# Authorize the key for testuser
echo "==> Authorizing SSH key for testuser..."
cat /ssh-keys/id_ed25519.pub >> /home/testuser/.ssh/authorized_keys
chmod 600 /home/testuser/.ssh/authorized_keys
chown testuser:testuser /home/testuser/.ssh/authorized_keys

echo "==> Starting SSH server..."
exec /usr/sbin/sshd -D -e
