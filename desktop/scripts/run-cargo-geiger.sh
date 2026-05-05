#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
artifact_dir="${repo_root}/artifacts"
report_path="${artifact_dir}/cargo-geiger.json"
log_path="${artifact_dir}/cargo-geiger.log"
bin_dir="${repo_root}/.tools/cargo-tools/bin"

mkdir -p "${artifact_dir}"
"${repo_root}/desktop/scripts/install-rust-quality-tool.sh" cargo-geiger
export PATH="${bin_dir}:${PATH}"

set +e
(
  cd "${repo_root}/desktop/src-tauri"
  cargo geiger -q --output-format Json --all-features --all-targets > "${report_path}"
) 2> "${log_path}"
status=$?
set -e

if [[ ${status} -ne 0 ]]; then
  if [[ -s "${report_path}" ]]; then
    printf 'cargo-geiger exited non-zero but produced a report; preserving the report without failing the command.\n'
  elif grep -q "error: Found " "${log_path}"; then
    grep "error: Found " "${log_path}" | tail -n 1
    printf 'cargo-geiger reported unsafe usage; preserving the report without failing the command.\n'
  else
    printf 'cargo-geiger exited with status %s before producing a report; preserving diagnostics without failing the command.\n' "${status}"
  fi
fi

printf 'Wrote %s\n' "${report_path}"
printf 'Captured cargo-geiger diagnostics in %s\n' "${log_path}"
