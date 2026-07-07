#!/usr/bin/env bash
# Remove build artifacts: the cargo target/ dir and the packaged dist/ bundle.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

cargo clean
rm -rf "${DIST}"

printf 'cleaned target/ and %s\n' "${DIST}"
