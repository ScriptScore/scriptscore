#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
coverage_dir="${repo_root}/artifacts/coverage"
bin_dir="${repo_root}/.tools/cargo-tools/bin"

if ! command -v pkg-config >/dev/null 2>&1; then
  cat >&2 <<'EOF'
error: pkg-config is required for desktop Rust coverage.

Install the Tauri build dependencies for your platform, then retry:
  Fedora: sudo dnf install dbus-devel pkgconf-pkg-config
  Ubuntu: sudo apt install libdbus-1-dev pkg-config
EOF
  exit 1
fi

if ! pkg-config --exists dbus-1; then
  cat >&2 <<'EOF'
error: dbus-1 development files are required for desktop Rust coverage.

The libdbus-sys crate needs dbus-1.pc to compile the same desktop dependency
graph that CI compiles.

Install the missing package, then retry:
  Fedora: sudo dnf install dbus-devel pkgconf-pkg-config
  Ubuntu: sudo apt install libdbus-1-dev pkg-config
EOF
  exit 1
fi

mkdir -p "${coverage_dir}"
"${repo_root}/desktop/scripts/install-rust-quality-tool.sh" cargo-tarpaulin
export PATH="${bin_dir}:${PATH}"

if [[ -z "${SCRIPTSCORE_PYTHON:-}" ]]; then
  unix_python="${repo_root}/cli/.venv/bin/python"
  windows_python="${repo_root}/cli/.venv/Scripts/python.exe"
  if [[ -x "${unix_python}" ]]; then
    export SCRIPTSCORE_PYTHON="${unix_python}"
  elif [[ -x "${windows_python}" ]]; then
    export SCRIPTSCORE_PYTHON="${windows_python}"
  fi
fi

(
  cd "${repo_root}/desktop/src-tauri"
  cargo tarpaulin \
    --workspace \
    --all-features \
    --all-targets \
    --engine llvm \
    --out Lcov \
    --out Xml \
    --output-dir "${coverage_dir}"
)

printf 'Wrote %s\n' "${coverage_dir}"
