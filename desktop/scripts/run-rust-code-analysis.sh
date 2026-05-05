#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
artifact_dir="${repo_root}/artifacts"
report_path="${artifact_dir}/rust-code-analysis.json"
summary_path="${artifact_dir}/rust-code-analysis-summary.json"
binary_path="${repo_root}/.tools/cargo-tools/bin/rust-code-analysis-cli"
report_builder="${repo_root}/desktop/scripts/rust_code_analysis_report.py"

mkdir -p "${artifact_dir}"
"${repo_root}/desktop/scripts/install-rust-quality-tool.sh" rust-code-analysis-cli

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

src_raw="${tmp_dir}/src.json"
tests_raw="${tmp_dir}/tests.json"
tool_version="$("${binary_path}" --version | awk '{print $2}')"

"${binary_path}" -m -p "${repo_root}/desktop/src-tauri/src" -I "*.rs" -O json > "${src_raw}"
"${binary_path}" -m -p "${repo_root}/desktop/src-tauri/tests" -I "*.rs" -O json > "${tests_raw}"

python3 "${report_builder}" \
  --src-input "${src_raw}" \
  --tests-input "${tests_raw}" \
  --report-output "${report_path}" \
  --summary-output "${summary_path}" \
  --tool-version "${tool_version}"

printf 'Wrote %s\n' "${report_path}"
printf 'Wrote %s\n' "${summary_path}"
