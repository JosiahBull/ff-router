#!/usr/bin/env bash
# One-line installer for firefox-link-router.
#
# Downloads the prebuilt interactive installer from the latest GitHub release
# and runs it. The installer then downloads the matching `ff-router` binary,
# assembles "Firefox Router.app", and walks you through setting it up.
#
# Run straight from the web (nothing to clone):
#   bash -c "$(curl -fsSL https://raw.githubusercontent.com/josiahbull/ff-router/main/scripts/install.sh)"
#
# For a local development build instead, use scripts/dev-install.sh.
set -euo pipefail

REPO="josiahbull/ff-router"
ASSET="ff-router-installer"

if [ "$(uname -s)" != "Darwin" ]; then
  echo "error: firefox-link-router is macOS-only (found $(uname -s))." >&2
  exit 1
fi

url="https://github.com/${REPO}/releases/latest/download/${ASSET}"

tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT
bin="${tmp}/${ASSET}"

echo "Downloading the installer from ${url} ..."
curl -fsSL --retry 3 -o "${bin}" "${url}"
chmod +x "${bin}"

"${bin}" </dev/tty
