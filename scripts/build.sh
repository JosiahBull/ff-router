#!/usr/bin/env bash

source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

RUSTFLAGS="${RUSTFLAGS:-} -Zunstable-options -Cpanic=immediate-abort" \
  cargo build --release --target "${TARGET}" -Z build-std=std,panic_abort

printf 'built %s (%s)\n' "${RELEASE_BIN}" "$(du -h "${RELEASE_BIN}" | cut -f1)"
