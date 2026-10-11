#!/usr/bin/env bash
# Third-party detection model and photograph remain outside the Git checkout.
# The model has AGPL-3.0 obligations; see docs/object-detection.md.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${1:-${ROOT}/benchmarks/datasets/detection}"
mkdir -p "${DEST}"
MODEL="${DEST}/yolov8n.onnx"
IMAGE="${DEST}/bus.jpg"
MODEL_SHA256="190ba5f1e61411a001683e349d6b2cdb0804c0dc67a5e34cd8ff6fd00ee54b4d"
IMAGE_GIT_BLOB="40eaaf5c330d0c498fbe1dcacf9bb8bf566797fe"

verify_model() {
  printf '%s  %s\n' "${MODEL_SHA256}" "${MODEL}" | shasum -a 256 -c - >/dev/null 2>&1
}
verify_image() {
  [[ "$(git hash-object "${IMAGE}")" == "${IMAGE_GIT_BLOB}" ]]
}
download() {
  local url="$1" dest="$2"
  local tmp="${dest}.part"
  rm -f "${tmp}"
  curl --fail --location --retry 3 --retry-delay 2 \
    --connect-timeout 20 --max-time 300 --output "${tmp}" "${url}"
  mv "${tmp}" "${dest}"
}

if ! verify_model; then
  echo "Downloading externally hosted YOLOv8n ONNX (AGPL-3.0) ..."
  download "https://huggingface.co/webml/yolov8n/resolve/main/onnx/yolov8n.onnx" "${MODEL}"
fi
verify_model || { echo "ERROR: YOLOv8n SHA256 mismatch" >&2; exit 1; }

if ! verify_image; then
  echo "Downloading Ultralytics sample photograph ..."
  download "https://raw.githubusercontent.com/ultralytics/assets/main/im/bus.jpg" "${IMAGE}"
fi
verify_image || { echo "ERROR: bus.jpg upstream Git blob hash mismatch" >&2; exit 1; }

echo "Verified YOLOv8n model: ${MODEL}"
echo "Verified example photograph: ${IMAGE}"
