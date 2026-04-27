#!/usr/bin/env bash
set -euo pipefail

UV_VERSION="0.11.7"
UV_SHA256_SUM_SHA256="2d56c5c54e3027c2c26e4f0cc1383be99a7af0a6b39dc7f2a5c6f2e5aa8878e4"
UV_BASE_URL="https://github.com/astral-sh/uv/releases/download/${UV_VERSION}"

usage() {
  cat <<'USAGE'
Usage: scripts/bootstrap-uv.sh [--force] [--print-plan]

Download a pinned uv executable into Tentgent's bootstrap cache.

This is installer-owned bootstrap plumbing. It does not install uv globally and
does not require uv to already exist on the user's PATH.

Options:
  --force       Re-download and replace the cached executable.
  --print-plan  Print resolved paths and URLs without downloading.
  -h, --help    Show this help.

Environment:
  TENTGENT_HOME                 Override Tentgent runtime home.
  TENTGENT_BOOTSTRAP_CACHE_DIR  Override bootstrap cache directory.
  TENTGENT_BOOTSTRAP_TARGET     Override target triple for testing.
USAGE
}

fail() {
  echo "error: $*" >&2
  exit 1
}

require_command() {
  local name="$1"
  if ! command -v "${name}" >/dev/null 2>&1; then
    fail "required command not found: ${name}"
  fi
}

checksum_file() {
  local path="$1"
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "${path}" | awk '{print $1}'
    return
  fi
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${path}" | awk '{print $1}'
    return
  fi
  fail "shasum or sha256sum is required"
}

verify_sha256() {
  local path="$1"
  local expected="$2"
  local actual
  actual="$(checksum_file "${path}")"
  if [[ "${actual}" != "${expected}" ]]; then
    fail "checksum mismatch for ${path}: expected ${expected}, got ${actual}"
  fi
}

default_runtime_home() {
  if [[ -n "${TENTGENT_HOME:-}" ]]; then
    echo "${TENTGENT_HOME}"
    return
  fi

  case "$(uname -s)" in
    Darwin) echo "${HOME}/Library/Application Support/com.tentserv.tentgent" ;;
    Linux) echo "${XDG_DATA_HOME:-${HOME}/.local/share}/tentgent" ;;
    *) fail "TENTGENT_HOME is required on this platform" ;;
  esac
}

host_target() {
  local os
  local arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "${os}:${arch}" in
    Darwin:arm64) echo "aarch64-apple-darwin" ;;
    Darwin:x86_64) echo "x86_64-apple-darwin" ;;
    Linux:aarch64) echo "aarch64-unknown-linux-gnu" ;;
    Linux:x86_64) echo "x86_64-unknown-linux-gnu" ;;
    *) fail "unsupported bootstrap target: ${os}:${arch}" ;;
  esac
}

asset_for_target() {
  local target="$1"
  case "${target}" in
    aarch64-apple-darwin) echo "uv-aarch64-apple-darwin.tar.gz" ;;
    x86_64-apple-darwin) echo "uv-x86_64-apple-darwin.tar.gz" ;;
    aarch64-unknown-linux-gnu) echo "uv-aarch64-unknown-linux-gnu.tar.gz" ;;
    x86_64-unknown-linux-gnu) echo "uv-x86_64-unknown-linux-gnu.tar.gz" ;;
    *) fail "unsupported uv bootstrap target: ${target}" ;;
  esac
}

expected_asset_sha256() {
  local sums_path="$1"
  local asset="$2"
  local line
  line="$(awk -v asset="${asset}" '$2 == "*" asset || $2 == asset { print; exit }' "${sums_path}")"
  if [[ -z "${line}" ]]; then
    fail "checksum entry not found for ${asset}"
  fi
  echo "${line}" | awk '{print $1}'
}

download_file() {
  local url="$1"
  local destination="$2"
  curl --proto '=https' --tlsv1.2 -fL "${url}" -o "${destination}"
}

print_plan() {
  cat <<PLAN
uv version: ${UV_VERSION}
target: ${TARGET}
asset: ${ASSET}
archive url: ${ARCHIVE_URL}
checksum url: ${SUMS_URL}
cache dir: ${TOOL_DIR}
uv path: ${UV_PATH}
manifest: ${MANIFEST_PATH}
PLAN
}

FORCE="false"
PRINT_PLAN="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --force)
      FORCE="true"
      shift
      ;;
    --print-plan)
      PRINT_PLAN="true"
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

TARGET="${TENTGENT_BOOTSTRAP_TARGET:-$(host_target)}"
ASSET="$(asset_for_target "${TARGET}")"
RUNTIME_HOME="$(default_runtime_home)"
CACHE_DIR="${TENTGENT_BOOTSTRAP_CACHE_DIR:-${RUNTIME_HOME}/runtime/bootstrap}"
TOOL_DIR="${CACHE_DIR}/uv/${UV_VERSION}/${TARGET}"
UV_PATH="${TOOL_DIR}/bin/uv"
MANIFEST_PATH="${TOOL_DIR}/manifest.toml"
ARCHIVE_URL="${UV_BASE_URL}/${ASSET}"
SUMS_URL="${UV_BASE_URL}/sha256.sum"

if [[ "${PRINT_PLAN}" == "true" ]]; then
  print_plan
  exit 0
fi

require_command curl
require_command tar
require_command grep
require_command awk

if [[ -x "${UV_PATH}" && "${FORCE}" != "true" ]]; then
  echo "==> Pinned uv already cached"
  echo "${UV_PATH}"
  exit 0
fi

TMP_DIR="${TOOL_DIR}/.tmp.$$"
SUMS_PATH="${TMP_DIR}/sha256.sum"
ARCHIVE_PATH="${TMP_DIR}/${ASSET}"
EXTRACT_DIR="${TMP_DIR}/extract"

rm -rf "${TMP_DIR}"
mkdir -p "${EXTRACT_DIR}" "${TOOL_DIR}/bin"

cleanup() {
  rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

echo "==> Downloading uv checksum manifest ${UV_VERSION}"
download_file "${SUMS_URL}" "${SUMS_PATH}"
verify_sha256 "${SUMS_PATH}" "${UV_SHA256_SUM_SHA256}"

EXPECTED_ASSET_SHA256="$(expected_asset_sha256 "${SUMS_PATH}" "${ASSET}")"

echo "==> Downloading pinned uv ${UV_VERSION} for ${TARGET}"
download_file "${ARCHIVE_URL}" "${ARCHIVE_PATH}"
verify_sha256 "${ARCHIVE_PATH}" "${EXPECTED_ASSET_SHA256}"

echo "==> Extracting uv"
tar -xzf "${ARCHIVE_PATH}" -C "${EXTRACT_DIR}"

EXTRACTED_UV="$(find "${EXTRACT_DIR}" -type f -name uv | head -n 1)"
if [[ -z "${EXTRACTED_UV}" ]]; then
  fail "uv executable was not found in ${ASSET}"
fi

cp "${EXTRACTED_UV}" "${UV_PATH}"
chmod 0755 "${UV_PATH}"
cp "${SUMS_PATH}" "${TOOL_DIR}/sha256.sum"

cat >"${MANIFEST_PATH}" <<MANIFEST
tool = "uv"
version = "${UV_VERSION}"
target = "${TARGET}"
asset = "${ASSET}"
url = "${ARCHIVE_URL}"
sha256 = "${EXPECTED_ASSET_SHA256}"
checksum_manifest_url = "${SUMS_URL}"
checksum_manifest_sha256 = "${UV_SHA256_SUM_SHA256}"
uv_path = "${UV_PATH}"
MANIFEST

echo "==> Pinned uv cached"
echo "${UV_PATH}"
