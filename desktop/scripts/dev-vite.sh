#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DESKTOP_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
FRONTEND_DIR="${DESKTOP_DIR}/frontend"
PORT="${VITE_PORT:-5173}"
HOST="${VITE_HOST:-127.0.0.1}"

if ! command -v npm >/dev/null 2>&1; then
  echo "error: npm is required to launch the desktop frontend dev server." >&2
  echo "Install Node.js and npm, then run this script again." >&2
  exit 1
fi

if [[ ! -f "${FRONTEND_DIR}/package.json" ]]; then
  echo "error: could not find desktop/frontend/package.json." >&2
  exit 1
fi

if [[ ! -d "${FRONTEND_DIR}/node_modules" ]]; then
  echo "error: frontend dependencies are not installed." >&2
  echo "Run: cd ${FRONTEND_DIR} && npm install" >&2
  exit 1
fi

cd "${FRONTEND_DIR}"
exec npm run dev -- --host "${HOST}" --port "${PORT}"
