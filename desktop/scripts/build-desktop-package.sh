#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd -- "${SCRIPT_DIR}/../.." && pwd)
DESKTOP_ROOT="${PROJECT_ROOT}/desktop"
BUNDLES="${SCRIPTSCORE_DESKTOP_BUNDLES:-appimage}"
TARGET="${SCRIPTSCORE_DESKTOP_TARGET:-}"
AUTO_PORTABLE_PYTHON="${SCRIPTSCORE_DESKTOP_AUTO_PORTABLE_PYTHON:-1}"
PORTABLE_PYTHON_DIR="${SCRIPTSCORE_DESKTOP_PORTABLE_PYTHON_DIR:-${DESKTOP_ROOT}/dist/portable-python}"

if [[ $# -gt 0 ]]; then
  BUNDLES=$1
fi

bundle_selection_includes() {
  local selected=$1
  local needle=$2
  local normalized

  if [[ -z "${selected}" ]]; then
    return 1
  fi

  if [[ "${selected}" == "all" || "${selected}" == "${needle}" ]]; then
    return 0
  fi

  normalized=",${selected// /},"
  [[ "${normalized}" == *",${needle},"* ]]
}

bundle_selection_requires_portable_runtime() {
  local bundle
  for bundle in app deb dmg msi nsis rpm; do
    if bundle_selection_includes "${BUNDLES}" "${bundle}"; then
      if [[ "${bundle}" == "appimage" && "${BUNDLES}" == "appimage" ]]; then
        continue
      fi
      return 0
    fi
  done
  return 1
}

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo is required to package the desktop app." >&2
  exit 1
fi

if ! cargo tauri --help >/dev/null 2>&1; then
  echo "error: cargo-tauri is required. Install it with 'cargo install tauri-cli --version ^2'." >&2
  exit 1
fi

if bundle_selection_includes "${BUNDLES}" "appimage"; then
  missing_tools=()

  for tool in mksquashfs patchelf; do
    if ! command -v "${tool}" >/dev/null 2>&1; then
      missing_tools+=("${tool}")
    fi
  done

  if [[ ${#missing_tools[@]} -gt 0 ]]; then
    echo "error: AppImage packaging requires the following host tools: ${missing_tools[*]}" >&2
    echo "Install the Linux packaging prerequisites documented in desktop/README.md and retry." >&2
    exit 1
  fi
fi

if [[ -z "${SCRIPTSCORE_DESKTOP_PORTABLE_PYTHON_ROOT:-}" ]] && [[ "${AUTO_PORTABLE_PYTHON}" != "0" ]]; then
  if bundle_selection_requires_portable_runtime; then
    bash "${SCRIPT_DIR}/prepare-portable-python.sh"
    export SCRIPTSCORE_DESKTOP_PORTABLE_PYTHON_ROOT="${PORTABLE_PYTHON_DIR}"
    echo "Using portable Python root: ${SCRIPTSCORE_DESKTOP_PORTABLE_PYTHON_ROOT}"
  fi
fi

if [[ "$(uname -s)" == "Linux" ]] && bundle_selection_includes "${BUNDLES}" "appimage"; then
  bundled_python_lib="${DESKTOP_ROOT}/dist/bundled-runtime/python/lib"
  bundled_python_library_paths=()
  if [[ -d "${bundled_python_lib}" ]]; then
    bundled_python_library_paths+=("${bundled_python_lib}")
    while IFS= read -r wheel_library_dir; do
      bundled_python_library_paths+=("${wheel_library_dir}")
    done < <(find "${bundled_python_lib}" -type d -name "*.libs" | sort)

    old_ifs=${IFS}
    IFS=:
    export LD_LIBRARY_PATH="${bundled_python_library_paths[*]}${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"
    IFS=${old_ifs}
  fi
fi

build_cmd=(
  cargo
  tauri
  build
  --config
  "${DESKTOP_ROOT}/src-tauri/tauri.conf.json"
)

if [[ -n "${BUNDLES}" ]]; then
  build_cmd+=(--bundles "${BUNDLES}")
fi

if [[ -n "${TARGET}" ]]; then
  build_cmd+=(--target "${TARGET}")
fi

if [[ "${SCRIPTSCORE_DESKTOP_VERBOSE_TAURI:-0}" == "1" ]]; then
  build_cmd+=(--verbose)
fi

"${build_cmd[@]}"

echo "Desktop package build finished"
echo "Artifacts root: ${DESKTOP_ROOT}/src-tauri/target"
