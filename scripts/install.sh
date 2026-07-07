#!/usr/bin/env bash
# Build and launch the interactive TUI installer. It discovers your Firefox
# profiles, helps you write ~/.ff-router.toml, installs "Firefox Router.app",
# and then removes its own build artifacts.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

cargo build -p ff-router-installer
exec "${REPO_ROOT}/target/debug/ff-router-installer" "$@"
