#!/bin/bash
# SessionStart hook for Claude Code cloud sessions: installs the system packages CI uses
# (.github/workflows/ci.yml) and warms the build in the background, so the ~20 minute
# OpenCASCADE build is under way (or done) by the time the first cargo command runs.
# Local sessions are left alone.
set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

cd "${CLAUDE_PROJECT_DIR:-$(dirname "$0")/../..}"

PKGS=(cmake g++ pkg-config libxkbcommon-x11-dev libxkbcommon-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev
  libwayland-dev libgl1-mesa-dev libegl1-mesa-dev)
missing=()
for p in "${PKGS[@]}"; do
  dpkg -s "$p" >/dev/null 2>&1 || missing+=("$p")
done
if [ ${#missing[@]} -gt 0 ]; then
  SUDO=""
  [ "$(id -u)" -ne 0 ] && SUDO="sudo"
  echo "session-start: installing ${missing[*]}"
  if ! { $SUDO apt-get update -qq && DEBIAN_FRONTEND=noninteractive $SUDO apt-get install -y -qq --no-install-recommends "${missing[@]}"; } >/dev/null 2>&1; then
    echo "session-start: apt-get install failed; the OCCT build may lack cmake or a C++ compiler"
  fi
fi

# Warm the build: the debug build is what `cargo test` and `cargo clippy` reuse (OCCT goes to
# target/OCCT whatever the profile). One warm-up at a time; follow it in target/warm-build.log.
mkdir -p target
if command -v flock >/dev/null; then
  nohup flock -n target/.warm-build.lock bash -c \
    'cargo build --workspace --all-targets && cargo clippy --workspace --all-targets' \
    >target/warm-build.log 2>&1 &
  echo "session-start: warming the build in the background (tail -f target/warm-build.log)"
fi
exit 0
