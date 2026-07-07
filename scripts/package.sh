#!/usr/bin/env bash
# Build and assemble a signed, self-contained "Firefox Router.app" in dist/.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

"${REPO_ROOT}/scripts/build.sh"

BUNDLE="${DIST}/${APP}"
rm -rf "${BUNDLE}"
mkdir -p "${BUNDLE}/Contents/MacOS"
cp "${REPO_ROOT}/Info.plist" "${BUNDLE}/Contents/Info.plist"
cp "${RELEASE_BIN}" "${BUNDLE}/Contents/MacOS/${BIN_NAME}"
printf 'APPL????' >"${BUNDLE}/Contents/PkgInfo"
codesign --force --sign - "${BUNDLE}"

printf 'packaged %s\n' "${BUNDLE}"
