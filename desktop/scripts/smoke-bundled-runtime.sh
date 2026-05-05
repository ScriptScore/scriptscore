#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd -- "${SCRIPT_DIR}/../.." && pwd)
OUTPUT_ROOT="${SCRIPTSCORE_DESKTOP_RUNTIME_DIR:-${PROJECT_ROOT}/desktop/dist/bundled-runtime}"
MANIFEST_PATH="${OUTPUT_ROOT}/runtime-manifest.json"

if [[ ! -f "${MANIFEST_PATH}" ]]; then
  echo "error: desktop runtime manifest was not found at ${MANIFEST_PATH}" >&2
  exit 1
fi

if command -v python3 >/dev/null 2>&1; then
  JSON_PYTHON=$(command -v python3)
elif command -v python >/dev/null 2>&1; then
  JSON_PYTHON=$(command -v python)
else
  echo "error: could not find python3 or python to run the bundled runtime smoke." >&2
  exit 1
fi

"${JSON_PYTHON}" - "${MANIFEST_PATH}" "${OUTPUT_ROOT}" <<'PY'
import json
import os
import subprocess
import sys

manifest_path, runtime_root = sys.argv[1:]
with open(manifest_path, "r", encoding="utf-8") as handle:
    manifest = json.load(handle)

python_executable = manifest["pythonExecutable"]
if not os.path.isabs(python_executable):
    python_executable = os.path.join(runtime_root, python_executable)

python_path_entries = [
    entry if os.path.isabs(entry) else os.path.join(runtime_root, entry)
    for entry in manifest.get("pythonPathEntries", [])
]

env = os.environ.copy()
if python_path_entries:
    existing = env.get("PYTHONPATH", "")
    env["PYTHONPATH"] = os.pathsep.join(
        python_path_entries + ([existing] if existing else [])
    )

subprocess.run(
    [
        python_executable,
        "-c",
        "import scriptscore; import scriptscore.transport.desktop_worker",
    ],
    check=True,
    env=env,
)
subprocess.run(
    [python_executable, "-m", "scriptscore.transport.desktop_worker", "--help"],
    check=True,
    env=env,
    stdout=subprocess.DEVNULL,
)
PY

echo "Desktop bundled runtime smoke passed"
