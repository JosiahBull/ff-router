#!/usr/bin/env bash
# Build everything up front, then launch the interactive installer. The TUI
# walks through each install action (write config, move the bundle, set
# permissions, register, clean up) and prompts before each one.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

cargo build -p ff-router-installer   # the installer TUI
"${REPO_ROOT}/scripts/package.sh"    # optimised ff-router binary + signed app bundle

exec "${REPO_ROOT}/target/debug/ff-router-installer" "$@"
