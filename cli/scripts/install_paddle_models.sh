#!/usr/bin/env bash

set -euo pipefail

DET_URL="https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv5_mobile_det_infer.tar"
REC_URL="https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv5_mobile_rec_infer.tar"

usage() {
  cat <<'EOF'
Usage:
  scripts/install_paddle_models.sh [DEST_DIR]

Arguments:
  DEST_DIR   Optional destination root for Paddle models.
             Defaults to models/paddle

Environment:
  PADDLE_MODEL_ROOT  Alternative way to set DEST_DIR.

The destination directory will be populated like:
  DEST_DIR/
    det/
      inference.yml
      inference.pdmodel
      inference.pdiparams
    rec/
      inference.yml
      inference.pdmodel
      inference.pdiparams
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

DEST_ROOT="${1:-${PADDLE_MODEL_ROOT:-models/paddle}}"
DET_DEST="${DEST_ROOT}/det"
REC_DEST="${DEST_ROOT}/rec"

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 1
  fi
}

download_file() {
  local url="$1"
  local out="$2"

  if command -v curl >/dev/null 2>&1; then
    curl -fL --retry 3 --retry-delay 2 -o "$out" "$url"
    return
  fi
  if command -v wget >/dev/null 2>&1; then
    wget -O "$out" "$url"
    return
  fi

  echo "missing download tool: install curl or wget" >&2
  exit 1
}

validate_model_dir() {
  local dir="$1"
  if [[ ! -f "${dir}/inference.yml" ]]; then
    echo "model install incomplete: missing ${dir}/inference.yml" >&2
    exit 1
  fi
  if [[ ! -f "${dir}/inference.pdmodel" && ! -f "${dir}/inference.json" ]]; then
    echo "model install incomplete: missing ${dir}/inference.pdmodel or ${dir}/inference.json" >&2
    exit 1
  fi
  if [[ ! -f "${dir}/inference.pdiparams" ]]; then
    echo "model install incomplete: missing ${dir}/inference.pdiparams" >&2
    exit 1
  fi
}

need_cmd tar
need_cmd mktemp

TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

DET_TAR="${TMP_DIR}/det.tar"
REC_TAR="${TMP_DIR}/rec.tar"

echo "Downloading Paddle detector model..."
download_file "${DET_URL}" "${DET_TAR}"

echo "Downloading Paddle recognizer model..."
download_file "${REC_URL}" "${REC_TAR}"

echo "Extracting archives..."
tar -xf "${DET_TAR}" -C "${TMP_DIR}"
tar -xf "${REC_TAR}" -C "${TMP_DIR}"

DET_SRC="$(find "${TMP_DIR}" -maxdepth 2 -type f -name inference.yml -path '*/PP-OCRv5_mobile_det_infer/*' -printf '%h\n' | head -n 1)"
REC_SRC="$(find "${TMP_DIR}" -maxdepth 2 -type f -name inference.yml -path '*/PP-OCRv5_mobile_rec_infer/*' -printf '%h\n' | head -n 1)"

if [[ -z "${DET_SRC}" || -z "${REC_SRC}" ]]; then
  echo "failed to find extracted Paddle model directories in downloaded archives" >&2
  exit 1
fi

mkdir -p "${DET_DEST}" "${REC_DEST}"

echo "Installing detector into ${DET_DEST}"
cp -f "${DET_SRC}/"* "${DET_DEST}/"

echo "Installing recognizer into ${REC_DEST}"
cp -f "${REC_SRC}/"* "${REC_DEST}/"

validate_model_dir "${DET_DEST}"
validate_model_dir "${REC_DEST}"

cat <<EOF
PaddleOCR models installed successfully.

Model root: ${DEST_ROOT}

Next steps for development test:
  cli/tests/test_scans_pii.py

Or export:
  export SCRIPTSCORE_TEST_PII_PADDLE_MODEL_DIR=${DEST_ROOT}

EOF
