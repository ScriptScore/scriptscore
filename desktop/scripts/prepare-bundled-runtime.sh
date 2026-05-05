#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd -- "${SCRIPT_DIR}/../.." && pwd)
DESKTOP_ROOT="${PROJECT_ROOT}/desktop"
OUTPUT_ROOT="${SCRIPTSCORE_DESKTOP_RUNTIME_DIR:-${DESKTOP_ROOT}/dist/bundled-runtime}"
CLI_SRC_ROOT="${PROJECT_ROOT}/cli/src"
PORTABLE_PYTHON_ROOT="${SCRIPTSCORE_DESKTOP_PORTABLE_PYTHON_ROOT:-}"
ABSOLUTE_PYTHON="${SCRIPTSCORE_DESKTOP_ABSOLUTE_PYTHON:-${SCRIPTSCORE_PYTHON:-}}"

detect_dev_python() {
  local repo_python
  repo_python="${PROJECT_ROOT}/cli/.venv/bin/python"
  if [[ -x "${repo_python}" ]]; then
    printf '%s\n' "${repo_python}"
    return 0
  fi

  repo_python="${PROJECT_ROOT}/cli/.venv/Scripts/python.exe"
  if [[ -x "${repo_python}" ]]; then
    printf '%s\n' "${repo_python}"
    return 0
  fi

  if command -v python3 >/dev/null 2>&1; then
    command -v python3
    return 0
  fi

  if command -v python >/dev/null 2>&1; then
    command -v python
    return 0
  fi

  return 1
}

detect_json_python() {
  if command -v python3 >/dev/null 2>&1; then
    command -v python3
    return 0
  fi
  if command -v python >/dev/null 2>&1; then
    command -v python
    return 0
  fi
  return 1
}

portable_python_relative_path() {
  local root=$1
  local candidate
  for candidate in "bin/python3" "bin/python" "python.exe" "Scripts/python.exe"; do
    if [[ -x "${root}/${candidate}" ]]; then
      printf 'python/%s\n' "${candidate}"
      return 0
    fi
  done
  return 1
}

if [[ -z "${ABSOLUTE_PYTHON}" ]]; then
  if ! ABSOLUTE_PYTHON=$(detect_dev_python); then
    echo "error: could not find a Python interpreter for the desktop runtime bundle." >&2
    echo "Set SCRIPTSCORE_DESKTOP_PORTABLE_PYTHON_ROOT for release packaging or SCRIPTSCORE_DESKTOP_ABSOLUTE_PYTHON for smoke packaging." >&2
    exit 1
  fi
fi

if ! JSON_PYTHON=$(detect_json_python); then
  echo "error: could not find python3 or python to write the runtime manifest." >&2
  exit 1
fi

rm -rf "${OUTPUT_ROOT}"
mkdir -p "${OUTPUT_ROOT}/cli-src"
cp -R "${CLI_SRC_ROOT}/scriptscore" "${OUTPUT_ROOT}/cli-src/scriptscore"

runtime_mode="absolute_python"
python_executable="${ABSOLUTE_PYTHON}"
portable_release=false

if [[ -n "${PORTABLE_PYTHON_ROOT}" ]]; then
  if [[ ! -d "${PORTABLE_PYTHON_ROOT}" ]]; then
    echo "error: SCRIPTSCORE_DESKTOP_PORTABLE_PYTHON_ROOT does not exist: ${PORTABLE_PYTHON_ROOT}" >&2
    exit 1
  fi
  mkdir -p "${OUTPUT_ROOT}/python"
  cp -R "${PORTABLE_PYTHON_ROOT}/." "${OUTPUT_ROOT}/python/"
  if ! python_executable=$(portable_python_relative_path "${PORTABLE_PYTHON_ROOT}"); then
    echo "error: portable Python root does not contain a supported interpreter layout." >&2
    exit 1
  fi
  runtime_mode="bundled_python"
  portable_release=true
fi

"${JSON_PYTHON}" - "${OUTPUT_ROOT}/runtime-manifest.json" "${python_executable}" "${runtime_mode}" "${portable_release}" <<'PY'
import json
import sys

manifest_path, python_executable, runtime_mode, portable_release = sys.argv[1:]
manifest = {
    "manifestVersion": 1,
    "runtimeMode": runtime_mode,
    "portableRelease": portable_release == "true",
    "pythonExecutable": python_executable,
    "pythonPathEntries": ["cli-src"],
    "scriptscorePlusPolicy": "optional_desktop_owned_only",
}
with open(manifest_path, "w", encoding="utf-8") as handle:
    json.dump(manifest, handle, indent=2, sort_keys=True)
    handle.write("\n")
PY

echo "Prepared desktop runtime bundle at ${OUTPUT_ROOT}"
if [[ "${portable_release}" == "true" ]]; then
  echo "Runtime mode: bundled portable Python"
else
  echo "Runtime mode: absolute Python smoke bundle (${python_executable})"
fi
