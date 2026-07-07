#!/usr/bin/env bash
# Remove the installed app, stop the login item, and deregister it from
# Launch Services.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

# Stop the resident instance and remove the login item. `bootout` is the modern
# replacement for the deprecated `unload` (which prints spurious errors on recent
# macOS); a not-loaded agent exits nonzero, which is fine here.
launchctl bootout "gui/$(id -u)/${BUNDLE_ID}" 2>/dev/null || true
rm -f "${LAUNCH_AGENT}"

"${LSREGISTER}" -u "${DEST}/${APP}" 2>/dev/null || true
rm -rf "${DEST:?}/${APP}"
printf 'removed %s\n' "${DEST}/${APP}"
