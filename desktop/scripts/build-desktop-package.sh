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
TAURI_CONFIG="${SCRIPTSCORE_DESKTOP_TAURI_CONFIG:-${DESKTOP_ROOT}/src-tauri/tauri.conf.json}"

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

detach_macos_dmg_interstitials() {
  local release_dir=$1
  local device
  while IFS= read -r device; do
    [[ -n "${device}" ]] || continue
    echo "Detaching stale macOS DMG interstitial device: ${device}"
    hdiutil detach -force "${device}" >/dev/null 2>&1 || true
  done < <(
    hdiutil info | awk -v prefix="${release_dir}/bundle/macos/rw." '
      /^image-path[[:space:]]*:/ {
        path = $0
        sub(/^image-path[[:space:]]*:[[:space:]]*/, "", path)
        in_target_image = index(path, prefix) == 1 && path ~ /\.dmg$/
        next
      }
      in_target_image && /^\/dev\/disk[0-9]+[[:space:]]/ {
        print $1
        in_target_image = 0
      }
    '
  )
}

create_plain_macos_dmg_fallback() {
  if [[ "${SCRIPTSCORE_DESKTOP_MACOS_PLAIN_DMG_FALLBACK:-0}" != "1" ]]; then
    return 1
  fi
  if [[ "$(uname -s)" != "Darwin" ]] || ! bundle_selection_includes "${BUNDLES}" "dmg"; then
    return 1
  fi

  local release_dir bundle_macos_dir dmg_dir app_bundle candidate
  if [[ -n "${TARGET}" ]]; then
    release_dir="${DESKTOP_ROOT}/src-tauri/target/${TARGET}/release"
  else
    release_dir="${DESKTOP_ROOT}/src-tauri/target/release"
  fi
  bundle_macos_dir="${release_dir}/bundle/macos"
  dmg_dir="${release_dir}/bundle/dmg"

  if [[ ! -d "${bundle_macos_dir}" ]]; then
    echo "warning: cannot create macOS DMG fallback; app bundle directory is missing: ${bundle_macos_dir}" >&2
    return 1
  fi

  app_bundle=""
  for candidate in "${bundle_macos_dir}"/*.app; do
    if [[ -d "${candidate}" ]]; then
      app_bundle="${candidate}"
      break
    fi
  done
  if [[ -z "${app_bundle}" ]]; then
    echo "warning: cannot create macOS DMG fallback; no .app bundle found in ${bundle_macos_dir}" >&2
    return 1
  fi

  local app_name product_name version arch_suffix dmg_name dmg_path tmp_dmg plist
  app_name=$(basename "${app_bundle}" .app)
  product_name="${app_name}"
  version=""
  plist="${app_bundle}/Contents/Info.plist"
  if [[ -f "${plist}" ]]; then
    product_name=$(/usr/libexec/PlistBuddy -c "Print :CFBundleName" "${plist}" 2>/dev/null || true)
    version=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "${plist}" 2>/dev/null || true)
  fi
  product_name="${product_name:-${app_name}}"
  version="${version:-0.0.0}"

  case "${TARGET:-$(uname -m)}" in
    x86_64-apple-darwin | x86_64)
      arch_suffix="x64"
      ;;
    aarch64-apple-darwin | arm64)
      arch_suffix="aarch64"
      ;;
    *)
      arch_suffix="$(uname -m)"
      ;;
  esac

  mkdir -p "${dmg_dir}"
  dmg_name="${app_name}_${version}_${arch_suffix}.dmg"
  dmg_path="${dmg_dir}/${dmg_name}"
  tmp_dmg="${dmg_dir}/.${dmg_name}.tmp.dmg"
  rm -f "${tmp_dmg}" "${dmg_path}"

  echo "warning: Tauri DMG bundling failed after producing ${app_bundle}; creating plain unsigned preview DMG." >&2
  detach_macos_dmg_interstitials "${release_dir}"
  if ! hdiutil create -volname "${product_name}" -srcfolder "${app_bundle}" -ov -format UDZO "${tmp_dmg}"; then
    rm -f "${tmp_dmg}"
    return 1
  fi
  if ! mv "${tmp_dmg}" "${dmg_path}"; then
    rm -f "${tmp_dmg}"
    return 1
  fi
  echo "Created fallback macOS DMG: ${dmg_path}"
  detach_macos_dmg_interstitials "${release_dir}"
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
    while IFS= read -r shared_library_dir; do
      bundled_python_library_paths+=("${shared_library_dir}")
    done < <(
      find "${bundled_python_lib}" -type f \( -name "*.so" -o -name "*.so.*" \) \
        -exec dirname {} \; |
        sort -u
    )

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
  "${TAURI_CONFIG}"
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

set +e
"${build_cmd[@]}"
build_status=$?
set -e

if [[ ${build_status} -ne 0 ]]; then
  if create_plain_macos_dmg_fallback; then
    build_status=0
  fi
fi

if [[ ${build_status} -ne 0 ]]; then
  exit "${build_status}"
fi

echo "Desktop package build finished"
echo "Artifacts root: ${DESKTOP_ROOT}/src-tauri/target"
