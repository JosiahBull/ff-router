#!/usr/bin/env bash
# Assert the project version is declared consistently everywhere it lives.
#
# Member crates use `version.workspace = true` (aligned by construction), but
# the workspace version in Cargo.toml and the hand-maintained Info.plist keys
# can drift. This checks they agree with each other, so a bump that touches one
# file but not the other fails at PR time rather than at release.
#
# When run on a git tag (GITHUB_REF_TYPE=tag, e.g. in the release workflow), it
# additionally asserts the tag names that same version — so a mistagged release
# fails before publishing.
#
# Run it anywhere: `./scripts/check-versions.sh`.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

ws=$(awk '/^\[workspace\.package\]/{f=1; next} f && /^version = /{gsub(/"/, "", $3); print $3; exit}' Cargo.toml)
cfver=$(awk '$0 ~ "<key>CFBundleVersion</key>"{getline; gsub(/.*<string>|<\/string>.*/, ""); print; exit}' Info.plist)
cfshort=$(awk '$0 ~ "<key>CFBundleShortVersionString</key>"{getline; gsub(/.*<string>|<\/string>.*/, ""); print; exit}' Info.plist)

echo "Cargo.toml workspace.package.version  = ${ws:-<empty>}"
echo "Info.plist CFBundleVersion            = ${cfver:-<empty>}"
echo "Info.plist CFBundleShortVersionString = ${cfshort:-<empty>}"

fail=0
# `::error::` renders as an annotation on GitHub; plain text elsewhere.
if [ -n "${GITHUB_ACTIONS:-}" ]; then
  err() { echo "::error::$*"; fail=1; }
else
  err() { echo "error: $*" >&2; fail=1; }
fi

[ -n "$ws" ] || err "could not read workspace.package.version from Cargo.toml"
[ "$ws" = "$cfver" ] || err "Info.plist CFBundleVersion '$cfver' does not match Cargo.toml version '$ws'"
[ "$ws" = "$cfshort" ] || err "Info.plist CFBundleShortVersionString '$cfshort' does not match Cargo.toml version '$ws'"

if [ "${GITHUB_REF_TYPE:-}" = "tag" ]; then
  tag="${GITHUB_REF_NAME#v}"
  [ "$ws" = "$tag" ] || err "tag '${GITHUB_REF_NAME}' does not match declared version '$ws'"
fi

if [ "$fail" -eq 0 ]; then
  echo "ok: versions agree"
fi
exit "$fail"
