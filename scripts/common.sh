# shellcheck shell=bash
# Shared configuration for the packaging scripts. Sourced, never executed.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

APP_NAME="Firefox Router"
APP="${APP_NAME}.app"
BIN_NAME="ff-router"
BUNDLE_ID="com.josiahbull.ff-router"
LAUNCH_AGENT="${HOME}/Library/LaunchAgents/${BUNDLE_ID}.plist"

DIST="${REPO_ROOT}/dist"
DEST="${HOME}/Applications"
# Default to the host triple, but let callers cross-compile by exporting TARGET
# (the release workflow builds both Apple arches this way, then lipos them).
TARGET="${TARGET:-$(rustc -vV | awk '/^host:/{print $2}')}"
RELEASE_BIN="${REPO_ROOT}/target/${TARGET}/release/${BIN_NAME}"
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
