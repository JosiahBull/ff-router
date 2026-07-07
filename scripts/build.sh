#!/usr/bin/env bash
# Build a size-optimised release binary.
#
# On top of the aggressive [profile.release] in Cargo.toml (opt-level=z, LTO,
# strip, panic=abort) this recompiles std with `build-std` and switches panics
# to `immediate-abort`, which drops panic formatting/backtrace machinery. The
# result is a small static binary with no launch-time decompression, so startup
# stays fast and resident memory is minimal.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

# build-std needs the std source; provision it quietly if missing.
rustup component add rust-src >/dev/null 2>&1 || true

RUSTFLAGS="${RUSTFLAGS:-} -Zunstable-options -Cpanic=immediate-abort" \
  cargo build --release --target "${TARGET}" -Z build-std=std,panic_abort

printf 'built %s (%s)\n' "${RELEASE_BIN}" "$(du -h "${RELEASE_BIN}" | cut -f1)"
