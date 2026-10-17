#!/usr/bin/env bash
set -euo pipefail
# Debian/Ubuntu; run as root in a development image.
apt-get update
apt-get install -y --no-install-recommends \
  build-essential pkg-config clang lld cmake ninja-build \
  libasound2-dev libfontconfig1-dev libfreetype6-dev \
  libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev \
  libxcb1-dev libxcb-shape0-dev libxcb-xfixes0-dev libxcb-randr0-dev \
  libxcb-render0-dev libxcb-shm0-dev libxcb-xinput-dev libxcb-xkb-dev \
  libvulkan-dev libssl-dev ffmpeg
