#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DESKTOP_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
HOST_DIR="${DESKTOP_DIR}/src-tauri"

export SCRIPTSCORE_DESKTOP_DIR="${DESKTOP_DIR}"

if [[ "$(uname -s)" == "Linux" ]]; then
  export WEBKIT_DISABLE_DMABUF_RENDERER="${WEBKIT_DISABLE_DMABUF_RENDERER:-1}"
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo is required to launch the Tauri desktop host." >&2
  echo "Install Rust and cargo, then run this script again." >&2
  exit 1
fi

if ! cargo tauri --help >/dev/null 2>&1; then
  echo "error: cargo-tauri is required to launch the desktop host." >&2
  echo "Install it with: cargo install tauri-cli --version '^2'" >&2
  exit 1
fi

if [[ ! -f "${HOST_DIR}/Cargo.toml" ]]; then
  echo "error: could not find desktop/src-tauri/Cargo.toml." >&2
  exit 1
fi

cd "${HOST_DIR}"

if [[ -n "${DESKTOP_FRONTEND_URL:-}" ]]; then
  TAURI_DEV_CONFIG="{\"build\":{\"beforeDevCommand\":\"bash ../scripts/dev-vite.sh\",\"devUrl\":\"${DESKTOP_FRONTEND_URL}\"},\"app\":{\"security\":{\"devCsp\":{\"img-src\":[\"'self'\",\"data:\",\"asset:\",\"${DESKTOP_FRONTEND_URL}\",\"http://asset.localhost\",\"https://asset.localhost\"]}}}}"
  exec cargo tauri dev --config "${TAURI_DEV_CONFIG}"
fi

exec cargo tauri dev
