#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

URL="${DESKTOP_FRONTEND_URL:-http://127.0.0.1:5173}"

find_browser() {
  local candidate
  for candidate in \
    google-chrome \
    chromium \
    chromium-browser \
    brave-browser \
    microsoft-edge \
    msedge
  do
    if command -v "${candidate}" >/dev/null 2>&1; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  done
  return 1
}

if ! BROWSER_CMD="$(find_browser)"; then
  echo "error: could not find a supported browser launcher." >&2
  echo "Install Chrome, Chromium, Brave, or Edge, then run this script again." >&2
  exit 1
fi

exec "${BROWSER_CMD}" --app="${URL}"
