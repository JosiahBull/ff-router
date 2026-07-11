#!/usr/bin/env bash

source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

"${REPO_ROOT}/scripts/build.sh"
cargo build -p ff-router-installer --release

FF_ROUTER_BIN="${RELEASE_BIN}" exec "${REPO_ROOT}/target/release/ff-router-installer" "$@"
