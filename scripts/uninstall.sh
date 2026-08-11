#!/usr/bin/env bash
# Remove the installed app, stop the login item, and deregister it from
# Launch Services.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

# Stop the resident instance and remove the login item.
launchctl bootout "gui/$(id -u)/${BUNDLE_ID}" 2>/dev/null || true
rm -f "${LAUNCH_AGENT}"

# Both the current location and the pre-.noindex one, so an uninstall run after
# upgrading still clears out an install that predates the move.
for dir in "${DEST}" "${LEGACY_DEST}"; do
  [ -d "${dir}/${APP}" ] || continue
  "${LSREGISTER}" -u "${dir}/${APP}" 2>/dev/null || true
  rm -rf "${dir:?}/${APP}"
  printf 'removed %s\n' "${dir}/${APP}"
done

# Leave ~/Applications alone, but tidy away our own now-empty subdirectory.
rmdir "${DEST}" 2>/dev/null || true
