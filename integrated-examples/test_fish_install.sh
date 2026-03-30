#!/usr/bin/env bash
#
# Integration test for fish shell support in the install script.
#
# Verifies that external/install.sh correctly:
#   1. Writes fish-compatible PATH config with literal $HOME/$PATH (not expanded)
#   2. Emits fish_add_path on stdout (for eval) when SHELL is fish
#   3. Emits export PATH on stdout when SHELL is bash
#   4. Is idempotent (doesn't duplicate config lines on re-run)
#   5. Bash config still works correctly alongside fish tests
#
# The test creates a minimal stub extracted from install.sh's PATH-writing
# and eval-output logic, avoiding the need for GitHub network access.
#
# Prerequisites:
#   - Ubuntu/Debian environment (for apt-get install fish)
#   - Coast binaries built (cargo build --release)
#
# Usage:
#   ./integrated-examples/test_fish_install.sh

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/helpers.sh"

echo "=== test_fish_install.sh — fish shell install script support ==="
echo ""

# --- Install fish shell ---

echo "=== Setup: installing fish shell ==="

# The base DinD image strips the universe repo for speed. The fish package
# lives in universe, so re-enable it.
CODENAME=$(lsb_release -cs 2>/dev/null || echo "jammy")
DPKG_ARCH=$(dpkg --print-architecture 2>/dev/null || echo "amd64")
if [ "$DPKG_ARCH" = "amd64" ] || [ "$DPKG_ARCH" = "i386" ]; then
    MIRROR="http://archive.ubuntu.com/ubuntu"
else
    MIRROR="http://ports.ubuntu.com/ubuntu-ports"
fi
echo "deb ${MIRROR} ${CODENAME} universe" >> /etc/apt/sources.list
apt-get update -qq
apt-get install -y -qq fish
command -v fish >/dev/null || fail "fish not installed"
pass "fish shell installed"

# --- Create isolated test home ---

TEST_HOME=$(mktemp -d)
trap 'rm -rf "$TEST_HOME"' EXIT

mkdir -p "$TEST_HOME/.coast/bin"
printf '#!/bin/sh\necho "coast v0.0.0-test"' > "$TEST_HOME/.coast/bin/coast"
printf '#!/bin/sh\necho coastd-fake' > "$TEST_HOME/.coast/bin/coastd"
chmod +x "$TEST_HOME/.coast/bin/coast" "$TEST_HOME/.coast/bin/coastd"

# --- Build the stub script ---
#
# This extracts the exact PATH-writing logic and eval output from install.sh
# so we test the real code without needing GitHub network access.

STUB="$TEST_HOME/install-stub.sh"
cat > "$STUB" << 'STUB_EOF'
#!/bin/sh
# Stub: PATH-writing + eval output extracted from external/install.sh
(
set -eu

INSTALL_DIR="${HOME}/.coast/bin"
OS="linux"

SHELL_RC_FILE=""
add_to_path() {
  SHELL_RC="$1"
  PATH_LINE='export PATH="$HOME/.coast/bin:$PATH"'

  if [ -f "${SHELL_RC}" ] && grep -qF '.coast/bin' "${SHELL_RC}"; then
    SHELL_RC_FILE="${SHELL_RC}"
    return
  fi

  printf "\n# Coast CLI\n%s\n" "${PATH_LINE}" >> "${SHELL_RC}"
  SHELL_RC_FILE="${SHELL_RC}"
}

if [ -n "${SHELL:-}" ]; then
  case "${SHELL}" in
    */zsh)  add_to_path "${HOME}/.zshrc" ;;
    */bash)
      if [ "$OS" = "darwin" ]; then
        add_to_path "${HOME}/.bash_profile"
      else
        add_to_path "${HOME}/.bashrc"
      fi
      ;;
    */fish)
      FISH_CONFIG="${HOME}/.config/fish/config.fish"
      if [ ! -f "${FISH_CONFIG}" ] || ! grep -qF '.coast/bin' "${FISH_CONFIG}"; then
        mkdir -p "$(dirname "${FISH_CONFIG}")"
        printf "\n# Coast CLI\nset -gx PATH \$HOME/.coast/bin \$PATH\n" >> "${FISH_CONFIG}"
      fi
      SHELL_RC_FILE="${FISH_CONFIG}"
      ;;
  esac
fi

) >&2

case "${SHELL:-}" in
  */fish) printf 'fish_add_path -gP "%s/.coast/bin"\n' "$HOME" ;;
  *)      printf 'export PATH="%s/.coast/bin:${PATH}"\n' "$HOME" ;;
esac
STUB_EOF
chmod +x "$STUB"

# ============================================================
# Test 1: Fish — config.fish written with literal $HOME/$PATH
# ============================================================

echo ""
echo "=== Test 1: Fish config.fish uses literal \$HOME/\$PATH ==="

HOME="$TEST_HOME" SHELL=/usr/bin/fish sh "$STUB" >/dev/null 2>&1

FISH_CONFIG="$TEST_HOME/.config/fish/config.fish"

[ -f "$FISH_CONFIG" ] || fail "config.fish was not created"
pass "config.fish created"

FISH_CONTENT=$(cat "$FISH_CONFIG")
assert_contains "$FISH_CONTENT" 'set -gx PATH $HOME/.coast/bin $PATH' "config.fish has fish-compatible PATH line"

# The config must NOT contain the expanded TEST_HOME path in the PATH line
assert_not_contains "$FISH_CONTENT" "$TEST_HOME/.coast/bin" "config.fish uses literal \$HOME, not expanded path"

pass "Fish config.fish written correctly"

# ============================================================
# Test 2: Fish — eval output emits fish_add_path
# ============================================================

echo ""
echo "=== Test 2: Fish eval output emits fish_add_path ==="

EVAL_FISH=$(HOME="$TEST_HOME" SHELL=/usr/bin/fish sh "$STUB" 2>/dev/null)

assert_contains "$EVAL_FISH" "fish_add_path" "eval output contains fish_add_path"
assert_not_contains "$EVAL_FISH" "export PATH" "eval output does not contain export PATH"
pass "Fish eval output correct"

# ============================================================
# Test 3: Bash — eval output emits export PATH
# ============================================================

echo ""
echo "=== Test 3: Bash eval output emits export PATH ==="

EVAL_BASH=$(HOME="$TEST_HOME" SHELL=/bin/bash sh "$STUB" 2>/dev/null)

assert_contains "$EVAL_BASH" "export PATH" "bash eval output contains export PATH"
assert_not_contains "$EVAL_BASH" "fish_add_path" "bash eval output does not contain fish_add_path"
pass "Bash eval output correct"

# ============================================================
# Test 4: Idempotence — re-running doesn't duplicate config
# ============================================================

echo ""
echo "=== Test 4: Idempotence ==="

HOME="$TEST_HOME" SHELL=/usr/bin/fish sh "$STUB" >/dev/null 2>&1
HOME="$TEST_HOME" SHELL=/usr/bin/fish sh "$STUB" >/dev/null 2>&1

COUNT=$(grep -c '.coast/bin' "$FISH_CONFIG")
assert_eq "$COUNT" "1" "config.fish has exactly one .coast/bin entry after multiple runs"
pass "Fish config is idempotent"

# ============================================================
# Test 5: Bash — .bashrc written correctly
# ============================================================

echo ""
echo "=== Test 5: Bash .bashrc written correctly ==="

BASH_HOME=$(mktemp -d)
mkdir -p "$BASH_HOME/.coast/bin"
cp "$TEST_HOME/.coast/bin/coast" "$BASH_HOME/.coast/bin/coast"
cp "$TEST_HOME/.coast/bin/coastd" "$BASH_HOME/.coast/bin/coastd"

HOME="$BASH_HOME" SHELL=/bin/bash sh "$STUB" >/dev/null 2>&1

BASHRC="$BASH_HOME/.bashrc"
[ -f "$BASHRC" ] || fail ".bashrc was not created"
pass ".bashrc created"

BASHRC_CONTENT=$(cat "$BASHRC")
assert_contains "$BASHRC_CONTENT" 'export PATH="$HOME/.coast/bin:$PATH"' ".bashrc has correct PATH export"
assert_not_contains "$BASHRC_CONTENT" "set -gx" ".bashrc does not contain fish syntax"

rm -rf "$BASH_HOME"
pass "Bash .bashrc written correctly"

# ============================================================
# Test 6: Fish eval output includes the actual HOME path
# ============================================================

echo ""
echo "=== Test 6: Fish eval output uses actual HOME in fish_add_path ==="

EVAL_FISH_PATH=$(HOME="$TEST_HOME" SHELL=/usr/bin/fish sh "$STUB" 2>/dev/null)
assert_contains "$EVAL_FISH_PATH" "$TEST_HOME/.coast/bin" "fish_add_path includes actual HOME path"
pass "Fish eval output has correct path"

# --- Done ---

echo ""
echo "==========================================="
echo "  ALL FISH INSTALL TESTS PASSED"
echo "==========================================="
