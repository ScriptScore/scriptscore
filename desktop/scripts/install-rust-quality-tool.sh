#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <cargo-package>" >&2
  exit 1
fi

tool_name="$1"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tools_root="${repo_root}/.tools/cargo-tools"
bin_dir="${tools_root}/bin"
stamp_dir="${repo_root}/.tools/tool-stamps"

case "${tool_name}" in
  cargo-geiger)
    version="0.13.0"
    install_args=(--features vendored-openssl)
    ;;
  cargo-tarpaulin)
    version="0.34.1"
    install_args=()
    ;;
  rust-code-analysis-cli)
    version="0.0.25"
    install_args=()
    ;;
  *)
    echo "unsupported tool: ${tool_name}" >&2
    exit 1
    ;;
esac

mkdir -p "${bin_dir}" "${stamp_dir}"

stamp_path="${stamp_dir}/${tool_name}-${version}.stamp"
if [[ -x "${bin_dir}/${tool_name}" && -f "${stamp_path}" ]]; then
  exit 0
fi

rm -f "${stamp_dir}/${tool_name}-"*.stamp

cargo install \
  --locked \
  --force \
  --root "${tools_root}" \
  "${tool_name}" \
  --version "${version}" \
  ${install_args+"${install_args[@]}"}

touch "${stamp_path}"
