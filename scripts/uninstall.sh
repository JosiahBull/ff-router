#!/usr/bin/env bash
# Remove the installed app and deregister it from Launch Services.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

"${LSREGISTER}" -u "${DEST}/${APP}" 2>/dev/null || true
rm -rf "${DEST:?}/${APP}"
printf 'removed %s\n' "${DEST}/${APP}"
