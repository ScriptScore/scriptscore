#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)

if command -v python3 >/dev/null 2>&1; then
  PYTHON=$(command -v python3)
elif command -v python >/dev/null 2>&1; then
  PYTHON=$(command -v python)
else
  echo "error: could not find python3 or python to prepare the portable desktop runtime." >&2
  exit 1
fi

exec "${PYTHON}" "${SCRIPT_DIR}/prepare_portable_python.py" "$@"
