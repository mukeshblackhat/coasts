# Remote Docker Runtime — POC

Run coast containers on a remote machine while keeping the daemon and CLI local.

## Problem

When multiple agents run in parallel, Docker containers exhaust the local machine's RAM.
This POC offloads all containers to a remote machine with more resources.

## Architecture

```
LOCAL MACHINE                         REMOTE MACHINE
┌─────────────────────┐               ┌─────────────────────┐
│  coastd (daemon)    │── HTTP ──────>│  coast-remote       │
│  coast (CLI)        │               │    ├─ /mount        │
│  Claude Code        │               │    ├─ /container/*  │
│                     │               │    └─ /status       │
│  NO containers      │               │                     │
│                     │   SSHFS       │  SSHFS mount        │
│  ~/myproject ───────┼──────────────>│  /mnt/coast/myapp   │
│                     │               │       │             │
│  localhost:8080 ◄───┼── SSH tunnel──│  Docker daemon      │
│                     │               │    └─ containers    │
└─────────────────────┘               │      └─ bind mount  │
                                      │         /mnt/coast/ │
                                      └─────────────────────┘
```

**Key insight**: SSHFS mounts the local project directory on the remote machine.
Docker on the remote machine bind-mounts from the SSHFS path. File changes are
reflected instantly — no re-sync needed.

## What's in this POC

### New crate: `coast-remote`
A standalone HTTP server that runs on the remote machine. Endpoints:

| Endpoint | Method | Purpose |
|---|---|---|
| `/api/v1/health` | GET | Health check (Docker connectivity) |
| `/api/v1/status` | GET | List all containers + mounts |
| `/api/v1/mount` | POST | Mount local project dir via SSHFS |
| `/api/v1/unmount` | POST | Unmount a project's SSHFS mount |
| `/api/v1/mounts` | GET | List active SSHFS mounts |
| `/api/v1/container/run` | POST | Create + start a DinD container |
| `/api/v1/container/stop` | POST | Stop a container |
| `/api/v1/container/rm` | POST | Remove a container |
| `/api/v1/container/exec` | POST | Execute a command in a container |
| `/api/v1/container/ip` | POST | Get container IP address |

### New module: `coast-docker/src/remote.rs`
`RemoteRuntime` — implements the `Runtime` trait by forwarding to coast-remote over HTTP.
Includes `mount_project()` / `unmount_project()` for SSHFS management.

### Wiring in `coast-daemon`
`AppState.remote_runtime` is set when `COAST_REMOTE_HOST` env var is present.
`state.is_remote()` helper for checking remote mode.

## Prerequisites

### On the remote machine
- Docker daemon running
- `sshfs` installed (`apt install sshfs` or `yum install fuse-sshfs`)
- SSH key access to the local machine (remote must be able to `ssh user@local`)
- Rust toolchain (to build coast-remote)

### On the local machine
- SSH server running (the remote machine connects back to mount your files)
- SSH key of the remote machine added to `~/.ssh/authorized_keys`

### SSH key setup (one time)

```bash
# On the remote machine, generate a key if needed
ssh-keygen -t ed25519

# Copy the remote's public key to local machine
ssh-copy-id user@local-machine

# Test it works (from remote)
ssh user@local-machine "echo ok"
```

## How to test

### Step 1: Build coast-remote on the remote machine

```bash
# On the remote machine
git clone <repo> && cd coasts
cargo build -p coast-remote
```

### Step 2: Start coast-remote

```bash
# On the remote machine
# Create mount directory (needs to exist and be writable)
sudo mkdir -p /mnt/coast && sudo chown $USER /mnt/coast

./target/debug/coast-remote --port 31416 --mount-dir /mnt/coast
# Output: "coast-remote listening on 0.0.0.0:31416"
```

### Step 3: Verify health from local machine

```bash
# From local machine (replace REMOTE_IP)
curl http://REMOTE_IP:31416/api/v1/health
# Expected: {"status":"ok","docker":"connected"}
```

### Step 4: Mount your project via SSHFS

```bash
# This tells the remote agent to sshfs your local project directory
curl -X POST http://REMOTE_IP:31416/api/v1/mount \
  -H "Content-Type: application/json" \
  -d '{
    "project": "myapp",
    "ssh_target": "youruser@LOCAL_IP",
    "remote_path": "/Users/youruser/projects/myapp"
  }'
# Expected: {"mount_path":"/mnt/coast/myapp","status":"mounted"}
```

Now `/mnt/coast/myapp` on the remote machine mirrors your local project directory.
Any file you edit locally shows up immediately on the remote.

### Step 5: Verify the mount

```bash
# On the remote machine, check the files are there
ls /mnt/coast/myapp/
# Should show your project files

# Or via the API
curl http://REMOTE_IP:31416/api/v1/mounts
```

### Step 6: Run a container

```bash
curl -X POST http://REMOTE_IP:31416/api/v1/container/run \
  -H "Content-Type: application/json" \
  -d '{
    "config": {
      "project": "myapp",
      "instance_name": "dev-1",
      "image": "docker:dind",
      "env_vars": {},
      "bind_mounts": [],
      "volume_mounts": [],
      "tmpfs_mounts": [],
      "networks": [],
      "working_dir": null,
      "entrypoint": null,
      "cmd": null,
      "labels": {},
      "published_ports": [{"host_port": 8080, "container_port": 8080}],
      "extra_hosts": []
    }
  }'
# Expected: {"container_id":"abc123..."}
```

The container now has your project files at `/host-project` via the SSHFS mount.

### Step 7: Check status

```bash
curl http://REMOTE_IP:31416/api/v1/status
# Shows containers and active SSHFS mounts
```

### Step 8: Access services via SSH tunnel

```bash
# From local machine: forward remote port 8080 to local
ssh -L 8080:localhost:8080 user@REMOTE_IP

# Now http://localhost:8080 reaches the container on the remote machine
```

### Step 9: Exec into the container

```bash
curl -X POST http://REMOTE_IP:31416/api/v1/container/exec \
  -H "Content-Type: application/json" \
  -d '{"project":"myapp","instance":"dev-1","cmd":["ls","/host-project"]}'
# Should show your project files inside the container
```

### Step 10: Edit locally, see changes remotely

```bash
# On your local machine, edit a file
echo "hello from local" >> ~/projects/myapp/test.txt

# Verify it shows up in the container
curl -X POST http://REMOTE_IP:31416/api/v1/container/exec \
  -H "Content-Type: application/json" \
  -d '{"project":"myapp","instance":"dev-1","cmd":["cat","/host-project/test.txt"]}'
# Should show "hello from local"
```

### Step 11: Cleanup

```bash
# Stop + remove the container
curl -X POST http://REMOTE_IP:31416/api/v1/container/stop \
  -H "Content-Type: application/json" \
  -d '{"project":"myapp","instance":"dev-1"}'

curl -X POST http://REMOTE_IP:31416/api/v1/container/rm \
  -H "Content-Type: application/json" \
  -d '{"project":"myapp","instance":"dev-1"}'

# Unmount the SSHFS mount
curl -X POST http://REMOTE_IP:31416/api/v1/unmount \
  -H "Content-Type: application/json" \
  -d '{"project":"myapp"}'
```

## Testing locally (single machine)

You can test on one machine by SSHFS-mounting from localhost:

```bash
# Terminal 1: start coast-remote
mkdir -p /tmp/coast-mounts
cargo run -p coast-remote -- --port 31416 --mount-dir /tmp/coast-mounts

# Terminal 2: mount and test
curl -X POST http://localhost:31416/api/v1/mount \
  -H "Content-Type: application/json" \
  -d '{
    "project": "myapp",
    "ssh_target": "'$USER'@localhost",
    "remote_path": "'$PWD'"
  }'

# Check it works
curl http://localhost:31416/api/v1/mounts
```

## Using with coast daemon (integrated mode)

```bash
# Set the env var before starting coastd
export COAST_REMOTE_HOST=http://REMOTE_IP:31416
coastd --foreground

# The daemon logs: "remote Docker runtime configured"
# state.is_remote() returns true
# Handlers can call state.remote_runtime.mount_project() before coast run
```

## Why SSHFS over tar/rsync

| | SSHFS | Tar upload | Rsync |
|---|---|---|---|
| Live file changes | Instant | Must re-upload | Must re-run |
| Custom code needed | None (system tool) | Upload endpoint | Sync logic |
| Bidirectional | Yes | No | Configurable |
| Performance | Good for dev | Good for CI | Good for large repos |
| Setup | SSH keys only | Nothing | SSH keys |

## Next steps (beyond POC)

1. **Auto-mount in daemon** — `coast run` calls `mount_project()` automatically
2. **SSH tunnel automation** — auto-setup port forwarding via SSH
3. **Streaming** — WebSocket endpoints for logs, exec TTY, stats
4. **Multi-host** — route different projects to different remote machines
5. **Auth/TLS** — secure the coast-remote API
6. **Reconnect handling** — auto-remount SSHFS on disconnect
7. **macOS FUSE** — reverse direction (mount remote on local) for macOS users without sshfs
