#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DESKTOP_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
FRONTEND_DIR="${DESKTOP_DIR}/frontend"
PORT="${VITE_PORT:-5173}"
HOST="${VITE_HOST:-127.0.0.1}"
URL="${DESKTOP_FRONTEND_URL:-http://${HOST}:${PORT}}"
OPEN_BROWSER="${DESKTOP_OPEN_BROWSER:-1}"
WAIT_SECONDS="${DESKTOP_FRONTEND_WAIT_SECONDS:-30}"

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

frontend_ready() {
  if command -v curl >/dev/null 2>&1; then
    curl --silent --fail --output /dev/null "${URL}"
    return $?
  fi
  if command -v wget >/dev/null 2>&1; then
    wget --quiet --spider "${URL}"
    return $?
  fi
  return 1
}

launch_browser() {
  local browser_cmd
  if [[ "${OPEN_BROWSER}" == "0" ]]; then
    return 0
  fi
  if ! browser_cmd="$(find_browser)"; then
    echo "warning: no supported browser launcher found; leaving the dev server running." >&2
    return 0
  fi
  "${browser_cmd}" --app="${URL}" >/dev/null 2>&1 &
  echo "Opened app-style browser window at ${URL}"
}

cleanup() {
  if [[ -n "${DEV_SERVER_PID:-}" ]] && kill -0 "${DEV_SERVER_PID}" >/dev/null 2>&1; then
    kill "${DEV_SERVER_PID}" >/dev/null 2>&1 || true
  fi
}

wait_for_frontend() {
  local attempt
  local attempts=$((WAIT_SECONDS * 2))
  for ((attempt = 1; attempt <= attempts; attempt += 1)); do
    if frontend_ready; then
      return 0
    fi
    sleep 0.5
  done
  return 1
}

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

if frontend_ready; then
  echo "Frontend is already responding at ${URL}."
  launch_browser
  exit 0
fi

cd "${FRONTEND_DIR}"
trap cleanup EXIT INT TERM
npm run dev -- --host "${HOST}" --port "${PORT}" &
DEV_SERVER_PID=$!

echo "Starting frontend dev server at ${URL} ..."
if ! wait_for_frontend; then
  echo "error: frontend dev server did not become ready within ${WAIT_SECONDS}s." >&2
  exit 1
fi

launch_browser
wait "${DEV_SERVER_PID}"
