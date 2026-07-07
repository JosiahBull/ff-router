#!/usr/bin/env bash
# Package, install to ~/Applications, and register with Launch Services.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

"${REPO_ROOT}/scripts/package.sh"

mkdir -p "${DEST}"
rm -rf "${DEST:?}/${APP}"
cp -R "${DIST}/${APP}" "${DEST}/${APP}"
"${LSREGISTER}" -f "${DEST}/${APP}"

cat <<EOF

Installed ${DEST}/${APP}
Now set it as your default browser:
  System Settings > Desktop & Dock > Default web browser > ${APP_NAME}
EOF
