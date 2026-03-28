#!/bin/bash
#
# Coast Remote — One-line VM setup
#
# Creates a Multipass VM with Docker + sshfs, builds coast-remote,
# sets up SSH keys for SSHFS, and starts the agent.
#
# Usage:
#   ./setup-remote.sh          # Create VM and start coast-remote
#   ./setup-remote.sh teardown  # Delete the VM
#   ./setup-remote.sh status    # Check if coast-remote is running
#   ./setup-remote.sh ssh       # SSH into the VM
#   ./setup-remote.sh stop      # Stop coast-remote (keep VM)
#   ./setup-remote.sh start     # Start coast-remote on existing VM
#
set -e

VM_NAME="coast-remote"
VM_CPUS=4
VM_MEMORY="8G"
VM_DISK="30G"
COAST_REMOTE_PORT=31416
MOUNT_DIR="/mnt/coast"
REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

info()  { echo -e "${CYAN}==> $1${NC}"; }
ok()    { echo -e "${GREEN}==> $1${NC}"; }
warn()  { echo -e "${YELLOW}==> $1${NC}"; }
err()   { echo -e "${RED}==> ERROR: $1${NC}" >&2; }

get_vm_ip() {
    multipass info "$VM_NAME" 2>/dev/null | grep IPv4 | awk '{print $2}'
}

get_local_ip() {
    # Try common interfaces
    local ip
    ip=$(ipconfig getifaddr en0 2>/dev/null) && [ -n "$ip" ] && echo "$ip" && return
    ip=$(ipconfig getifaddr en1 2>/dev/null) && [ -n "$ip" ] && echo "$ip" && return
    # Fallback: route-based detection
    ip=$(route get default 2>/dev/null | grep interface | awk '{print $2}')
    [ -n "$ip" ] && ipconfig getifaddr "$ip" 2>/dev/null && return
    err "Could not detect local IP. Set LOCAL_IP env var manually."
    exit 1
}

check_multipass() {
    if ! command -v multipass &>/dev/null; then
        err "Multipass not found. Install with: brew install multipass"
        exit 1
    fi
}

vm_exists() {
    multipass info "$VM_NAME" &>/dev/null
}

###############################################################################
# Commands
###############################################################################

cmd_teardown() {
    info "Deleting VM '$VM_NAME'..."
    multipass delete "$VM_NAME" --purge 2>/dev/null || true
    ok "VM deleted"

    # Clean up authorized_keys
    if [ -f ~/.ssh/authorized_keys ]; then
        sed -i '' "/$VM_NAME/d" ~/.ssh/authorized_keys 2>/dev/null || true
    fi
}

cmd_status() {
    if ! vm_exists; then
        echo "VM '$VM_NAME' does not exist"
        return 1
    fi

    local vm_ip
    vm_ip=$(get_vm_ip)
    echo "VM:   $VM_NAME ($vm_ip)"

    if curl -sf --max-time 3 "http://$vm_ip:$COAST_REMOTE_PORT/api/v1/health" >/dev/null 2>&1; then
        ok "coast-remote is running at http://$vm_ip:$COAST_REMOTE_PORT"
        echo ""
        echo "To use with coast daemon:"
        echo "  export COAST_REMOTE_HOST=http://$vm_ip:$COAST_REMOTE_PORT"
    else
        warn "coast-remote is NOT running"
        echo "Start it with: $0 start"
    fi
}

cmd_ssh() {
    multipass shell "$VM_NAME"
}

cmd_stop() {
    info "Stopping coast-remote on VM..."
    multipass exec "$VM_NAME" -- bash -c "pkill coast-remote 2>/dev/null || true"
    ok "coast-remote stopped"
}

cmd_start() {
    if ! vm_exists; then
        err "VM '$VM_NAME' does not exist. Run '$0' first to create it."
        exit 1
    fi

    local vm_ip
    vm_ip=$(get_vm_ip)

    info "Starting coast-remote on $vm_ip..."

    multipass exec "$VM_NAME" -- bash -c "
        pkill coast-remote 2>/dev/null || true
        sleep 1
        nohup /home/ubuntu/coasts/target/release/coast-remote \
            --port $COAST_REMOTE_PORT \
            --mount-dir $MOUNT_DIR \
            > /tmp/coast-remote.log 2>&1 &
        echo \$! > /tmp/coast-remote.pid
    "

    # Wait for it to be ready
    for i in $(seq 1 15); do
        if curl -sf --max-time 2 "http://$vm_ip:$COAST_REMOTE_PORT/api/v1/health" >/dev/null 2>&1; then
            ok "coast-remote running at http://$vm_ip:$COAST_REMOTE_PORT"
            echo ""
            echo "Use with coast daemon:"
            echo "  export COAST_REMOTE_HOST=http://$vm_ip:$COAST_REMOTE_PORT"
            return 0
        fi
        sleep 1
    done

    err "coast-remote failed to start. Check logs:"
    echo "  multipass exec $VM_NAME -- cat /tmp/coast-remote.log"
    exit 1
}

cmd_setup() {
    check_multipass

    local local_ip
    local_ip="${LOCAL_IP:-$(get_local_ip)}"
    local local_user
    local_user="$(whoami)"

    echo ""
    echo "╔══════════════════════════════════════════════════════╗"
    echo "║  Coast Remote — VM Setup                            ║"
    echo "╠══════════════════════════════════════════════════════╣"
    echo "║  VM:     $VM_NAME ($VM_CPUS CPU, $VM_MEMORY RAM)              ║"
    echo "║  Local:  $local_user@$local_ip                      "
    echo "╚══════════════════════════════════════════════════════╝"
    echo ""

    # Step 1: Create VM
    if vm_exists; then
        warn "VM '$VM_NAME' already exists, reusing it"
    else
        info "Creating VM with Docker (this takes ~2 minutes)..."
        multipass launch docker \
            --name "$VM_NAME" \
            --cpus "$VM_CPUS" \
            --memory "$VM_MEMORY" \
            --disk "$VM_DISK"
        ok "VM created"
    fi

    local vm_ip
    vm_ip=$(get_vm_ip)
    info "VM IP: $vm_ip"

    # Step 2: Install sshfs on VM
    info "Installing sshfs on VM..."
    multipass exec "$VM_NAME" -- sudo apt-get update -qq
    multipass exec "$VM_NAME" -- sudo apt-get install -y -qq sshfs > /dev/null
    multipass exec "$VM_NAME" -- sudo mkdir -p "$MOUNT_DIR"
    multipass exec "$VM_NAME" -- sudo chown ubuntu "$MOUNT_DIR"
    ok "sshfs installed"

    # Step 3: Copy repo to VM and build
    info "Syncing repo to VM (this takes a minute)..."

    # Create a tarball excluding heavy dirs
    local tarball="/tmp/coast-remote-sync.tar.gz"
    tar czf "$tarball" \
        -C "$REPO_DIR" \
        --exclude='target' \
        --exclude='.git' \
        --exclude='coast-guard' \
        --exclude='node_modules' \
        --exclude='integrated-examples/projects' \
        --exclude='.dindind-image-cache' \
        .

    multipass transfer "$tarball" "$VM_NAME":/tmp/coast-remote-sync.tar.gz
    multipass exec "$VM_NAME" -- bash -c "
        mkdir -p /home/ubuntu/coasts &&
        tar xzf /tmp/coast-remote-sync.tar.gz -C /home/ubuntu/coasts &&
        rm /tmp/coast-remote-sync.tar.gz
    "
    rm -f "$tarball"
    ok "Repo synced"

    info "Building coast-remote on VM (first build takes a few minutes)..."
    multipass exec "$VM_NAME" -- bash -c "
        if ! command -v cargo &>/dev/null; then
            curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        fi
        source ~/.cargo/env
        cd /home/ubuntu/coasts
        cargo build -p coast-remote --release 2>&1 | tail -3
    "
    ok "Build complete"

    # Step 4: SSH keys (VM → local Mac)
    info "Setting up SSH keys..."

    # Enable SSH on Mac (remind user)
    if ! pgrep -x sshd >/dev/null 2>&1; then
        warn "SSH server may not be running on your Mac."
        echo "  Enable it: System Settings → General → Sharing → Remote Login"
        echo "  Or run: sudo systemsetup -setremotelogin on"
        echo ""
        read -p "  Press Enter once SSH is enabled (or Ctrl-C to abort)..."
    fi

    # Generate key on VM if needed
    multipass exec "$VM_NAME" -- bash -c "
        [ -f ~/.ssh/id_ed25519 ] || ssh-keygen -t ed25519 -N '' -f ~/.ssh/id_ed25519 -q
    "

    # Add VM's public key to local authorized_keys
    local vm_pubkey
    vm_pubkey=$(multipass exec "$VM_NAME" -- cat /home/ubuntu/.ssh/id_ed25519.pub)

    mkdir -p ~/.ssh
    touch ~/.ssh/authorized_keys
    chmod 600 ~/.ssh/authorized_keys

    if ! grep -qF "$vm_pubkey" ~/.ssh/authorized_keys 2>/dev/null; then
        echo "$vm_pubkey # $VM_NAME" >> ~/.ssh/authorized_keys
        ok "VM's SSH key added to ~/.ssh/authorized_keys"
    else
        ok "VM's SSH key already authorized"
    fi

    # Test SSH from VM → Mac
    info "Testing SSH from VM to your Mac..."
    multipass exec "$VM_NAME" -- bash -c "
        ssh -o StrictHostKeyChecking=no -o ConnectTimeout=5 -o BatchMode=yes \
            $local_user@$local_ip 'echo SSH_OK' 2>/dev/null
    " && ok "SSH works: VM → Mac" || {
        err "SSH from VM to Mac failed."
        echo "  Make sure Remote Login is enabled in System Settings."
        echo "  Try manually: multipass exec $VM_NAME -- ssh $local_user@$local_ip"
        exit 1
    }

    # Step 5: Start coast-remote
    cmd_start

    echo ""
    echo "╔══════════════════════════════════════════════════════╗"
    echo "║  Setup Complete!                                    ║"
    echo "╠══════════════════════════════════════════════════════╣"
    echo "║                                                     ║"
    echo "  coast-remote: http://$vm_ip:$COAST_REMOTE_PORT"
    echo "║                                                     ║"
    echo "  To use with coast:                                  "
    echo "    export COAST_REMOTE_HOST=http://$vm_ip:$COAST_REMOTE_PORT"
    echo "║                                                     ║"
    echo "  To mount a project:                                 "
    echo "    curl -X POST http://$vm_ip:$COAST_REMOTE_PORT/api/v1/mount \\"
    echo "      -H 'Content-Type: application/json' \\"
    echo "      -d '{\"project\":\"myapp\",\"ssh_target\":\"$local_user@$local_ip\",\"remote_path\":\"'\"'\$PWD'\"'\"}'"
    echo "║                                                     ║"
    echo "  Other commands:                                     "
    echo "    $0 status    # Check coast-remote"
    echo "    $0 ssh       # Shell into VM"
    echo "    $0 stop      # Stop coast-remote"
    echo "    $0 start     # Restart coast-remote"
    echo "    $0 teardown  # Delete VM entirely"
    echo "║                                                     ║"
    echo "╚══════════════════════════════════════════════════════╝"
}

###############################################################################
# Main
###############################################################################

case "${1:-setup}" in
    setup)    cmd_setup ;;
    teardown) cmd_teardown ;;
    status)   cmd_status ;;
    ssh)      cmd_ssh ;;
    stop)     cmd_stop ;;
    start)    cmd_start ;;
    *)
        echo "Usage: $0 [setup|teardown|status|ssh|stop|start]"
        exit 1
        ;;
esac
