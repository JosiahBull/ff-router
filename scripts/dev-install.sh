#!/usr/bin/env bash
# Local development install: build everything from the current checkout and run
# the installer against the freshly-built binary instead of a GitHub release.
#
# The end-user path is scripts/install.sh (downloads prebuilt binaries).
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

# Size-optimised ff-router → ${RELEASE_BIN}; the installer copies this into the
# bundle when FF_ROUTER_BIN points at it (rather than downloading a release).
"${REPO_ROOT}/scripts/build.sh"
cargo build -p ff-router-installer --release

FF_ROUTER_BIN="${RELEASE_BIN}" exec "${REPO_ROOT}/target/release/ff-router-installer" "$@"
