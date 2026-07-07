#!/usr/bin/env bash
# Build everything up front, then launch the interactive installer. The TUI
# walks through each install action (write config, move the bundle, set
# permissions, register, clean up) and prompts before each one.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

cargo build -p ff-router-installer --release
"${REPO_ROOT}/scripts/package.sh"

exec "${REPO_ROOT}/target/release/ff-router-installer" "$@"
