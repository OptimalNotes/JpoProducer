#!/usr/bin/env bash
# JpoProducer — one-time WSL2 (Ubuntu) dev environment setup.
# Run inside Ubuntu: bash scripts/setup-wsl.sh

set -euo pipefail

export DEBIAN_FRONTEND=noninteractive

echo "==> apt packages (Rust + egui/winit + audio)..."
# From PowerShell (no sudo prompt): wsl -d Ubuntu -u root -e bash scripts/setup-wsl.sh
if [[ "$(id -u)" -ne 0 ]]; then
  sudo apt-get update -qq
  sudo apt-get install -y --no-install-recommends \
    build-essential pkg-config curl git ca-certificates \
    libssl-dev libasound2-dev libudev-dev \
    libxkbcommon-dev libwayland-dev libxrandr-dev \
    libxinerama-dev libxcursor-dev libxi-dev
else
  apt-get update -qq
  apt-get install -y --no-install-recommends \
    build-essential pkg-config curl git ca-certificates \
    libssl-dev libasound2-dev libudev-dev \
    libxkbcommon-dev libwayland-dev libxrandr-dev \
    libxinerama-dev libxcursor-dev libxi-dev
fi

if [[ "$(id -u)" -eq 0 ]]; then
  echo "Run the rest as your WSL user (not root): bash scripts/setup-wsl.sh"
  exit 0
fi

if ! command -v rustc >/dev/null 2>&1; then
  echo "==> rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env"

REPO="$HOME/JpoProducer"
if [[ -d "$REPO/.git" ]]; then
  echo "==> git pull $REPO"
  git -C "$REPO" pull --ff-only
else
  echo "==> clone JpoProducer -> $REPO"
  git clone https://github.com/OptimalNotes/JpoProducer.git "$REPO"
fi

SF2_WIN="/mnt/c/Users/user/JpoProducer/FluidR3 GM.SF2"
SF2_LOCAL="$REPO/FluidR3 GM.SF2"
if [[ ! -f "$SF2_LOCAL" && -f "$SF2_WIN" ]]; then
  echo "==> copy SF2 from Windows tree (one-time, ~140MB)..."
  cp "$SF2_WIN" "$SF2_LOCAL"
fi

if ! command -v grok >/dev/null 2>&1; then
  echo "==> grok CLI..."
  curl -fsSL https://x.ai/cli/install.sh | bash
fi

echo "==> cargo test (smoke)..."
cd "$REPO"
cargo test --quiet

cat <<'EOF'

Done.
  Project : ~/JpoProducer
  Grok    : grok          (first run may open browser auth)
  Build   : cargo run     (WSLg GUI on Windows 11)
  Tests   : cargo test

Tip: keep the repo on the Linux filesystem (~/JpoProducer), not /mnt/c — faster cargo.
EOF