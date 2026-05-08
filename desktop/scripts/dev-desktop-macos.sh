#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DESKTOP_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPO_DIR="$(cd "${DESKTOP_DIR}/.." && pwd)"
WORKSPACE_DIR="$(cd "${REPO_DIR}/.." && pwd)"
FRONTEND_DIR="${DESKTOP_DIR}/frontend"
HOST_DIR="${DESKTOP_DIR}/src-tauri"
PORT="${VITE_PORT:-5173}"
HOST="${VITE_HOST:-127.0.0.1}"
URL="${DESKTOP_FRONTEND_URL:-http://${HOST}:${PORT}}"
WAIT_SECONDS="${DESKTOP_FRONTEND_WAIT_SECONDS:-30}"

export SCRIPTSCORE_DESKTOP_DIR="${DESKTOP_DIR}"
export DESKTOP_FRONTEND_URL="${URL}"

find_workspace_node_bin() {
  local node_root
  for node_root in "${WORKSPACE_DIR}"/.tools/node-*-darwin-*/bin; do
    if [[ -x "${node_root}/npm" ]]; then
      printf '%s\n' "${node_root}"
      return 0
    fi
  done
  return 1
}

if [[ -n "${SCRIPTSCORE_NODE_BIN:-}" && -x "${SCRIPTSCORE_NODE_BIN}/npm" ]]; then
  export PATH="${SCRIPTSCORE_NODE_BIN}:${PATH}"
elif WORKSPACE_NODE_BIN="$(find_workspace_node_bin)"; then
  export PATH="${WORKSPACE_NODE_BIN}:${PATH}"
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: this launcher is only for macOS." >&2
  echo "Use desktop/scripts/dev-desktop.sh on Linux." >&2
  exit 1
fi

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: $1 is required to launch the macOS desktop app." >&2
    exit 1
  fi
}

frontend_ready() {
  curl --silent --fail --output /dev/null "${URL}"
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

cleanup() {
  if [[ -n "${DEV_SERVER_PID:-}" ]] && kill -0 "${DEV_SERVER_PID}" >/dev/null 2>&1; then
    kill "${DEV_SERVER_PID}" >/dev/null 2>&1 || true
  fi
}

need_cmd npm
need_cmd cargo
need_cmd curl

if ! cargo tauri --help >/dev/null 2>&1; then
  echo "error: cargo-tauri is required to launch the desktop host." >&2
  echo "Install it with: cargo install tauri-cli --version '^2'" >&2
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

if [[ ! -f "${HOST_DIR}/Cargo.toml" ]]; then
  echo "error: could not find desktop/src-tauri/Cargo.toml." >&2
  exit 1
fi

if frontend_ready; then
  echo "Frontend is already responding at ${URL}."
else
  cd "${FRONTEND_DIR}"
  trap cleanup EXIT INT TERM
  npm run dev -- --host "${HOST}" --port "${PORT}" &
  DEV_SERVER_PID=$!

  echo "Starting frontend dev server at ${URL} ..."
  if ! wait_for_frontend; then
    echo "error: frontend dev server did not become ready within ${WAIT_SECONDS}s." >&2
    exit 1
  fi
fi

TAURI_DEV_CONFIG="{\"build\":{\"beforeDevCommand\":\"\",\"devUrl\":\"${URL}\"},\"app\":{\"security\":{\"devCsp\":{\"img-src\":[\"'self'\",\"data:\",\"asset:\",\"${URL}\",\"http://asset.localhost\",\"https://asset.localhost\"]}}}}"

cd "${HOST_DIR}"
exec cargo tauri dev --config "${TAURI_DEV_CONFIG}"
