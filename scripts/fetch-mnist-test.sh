#!/usr/bin/env bash
# Download the canonical MNIST 10,000-image TEST split without committing it.
# Verify each compressed archive's published SHA256 before extraction.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${1:-${ROOT}/benchmarks/datasets/mnist}"
URL="https://storage.googleapis.com/cvdf-datasets/mnist"
mkdir -p "${DEST}"

fetch_verified() {
  local filename="$1"
  local expected="$2"
  local compressed="${DEST}/${filename}"
  local output="${DEST}/${filename%.gz}"

  if [[ ! -f "${compressed}" ]] ||
     ! (cd "${DEST}" && printf '%s  %s\n' "${expected}" "${filename}" | shasum -a 256 -c - >/dev/null 2>&1); then
    local temporary="${compressed}.part"
    trap 'rm -f "${temporary}"' RETURN
    echo "Downloading ${filename} ..."
    curl --fail --location --retry 3 --retry-delay 2 --connect-timeout 20 --max-time 120 \
      --output "${temporary}" "${URL}/${filename}"
    (cd "${DEST}" && printf '%s  %s\n' "${expected}" "${filename}.part" | shasum -a 256 -c -)
    mv "${temporary}" "${compressed}"
    trap - RETURN
  fi

  (cd "${DEST}" && printf '%s  %s\n' "${expected}" "${filename}" | shasum -a 256 -c -)
  if [[ ! -f "${output}" ]]; then
    local extract_tmp="${output}.part"
    gzip -dc "${compressed}" > "${extract_tmp}"
    mv "${extract_tmp}" "${output}"
  fi
}

# Canonical SHA256: see benchmarks/README.md and dataset provenance.
fetch_verified "t10k-images-idx3-ubyte.gz" "8d422c7b0a1c1c79245a5bcf07fe86e33eeafee792b84584aec276f5a2dbc4e6"
fetch_verified "t10k-labels-idx1-ubyte.gz" "f7ae60f92e00ec6debd23a6088c31dbd2371eca3ffa0defaefb259924204aec6"

echo "MNIST full test set ready: ${DEST}"
