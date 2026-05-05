#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)
DESKTOP_ROOT=$(cd -- "${SCRIPT_DIR}/.." && pwd)

"${SCRIPT_DIR}/prepare-bundled-runtime.sh"
npm --prefix "${DESKTOP_ROOT}/frontend" run build
"${SCRIPT_DIR}/generate_legal_artifacts.py"
