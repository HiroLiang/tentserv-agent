#!/usr/bin/env bash
set -euo pipefail

VERSION="0.3.0"
DEFAULT_BASE_URL="https://agent.tentserv.com/releases"

usage() {
  cat <<'USAGE'
Usage: scripts/install.sh [OPTIONS]

Install Tentgent from a versioned release tarball, then run the installer-owned
Python bootstrap and `tentgent doctor`.

Options:
  --archive <PATH_OR_URL>   Install from a local tarball, file:// URL, or HTTPS URL.
  --checksums <PATH_OR_URL> Verify with checksums.txt. Local archives infer same-dir checksums.txt.
  --version <VERSION>       Version to install when --archive is omitted.
  --prefix <PATH>           Install prefix. Defaults to ~/.local.
  --target <TARGET>         Override detected target triple.
  --dry-run                 Print the install plan without changing files.
  --skip-python-bootstrap   Install files but skip managed Python env bootstrap.
  --skip-doctor             Do not run `tentgent doctor` after installation.
  -h, --help                Show this help.

Environment:
  TENTGENT_INSTALL_BASE_URL  Override release base URL.
  TENTGENT_INSTALL_PREFIX    Override install prefix.
  TENTGENT_HOME              Override Tentgent runtime home.
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

host_target() {
  local os
  local arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "${os}:${arch}" in
    Darwin:arm64) echo "aarch64-apple-darwin" ;;
    Darwin:x86_64) echo "x86_64-apple-darwin" ;;
    *) fail "unsupported install target: ${os}:${arch}; pass --target to override" ;;
  esac
}

download_or_copy() {
  local source="$1"
  local destination="$2"

  case "${source}" in
    https://*)
      curl --proto '=https' --tlsv1.2 -fL "${source}" -o "${destination}"
      ;;
    file://*)
      cp "${source#file://}" "${destination}"
      ;;
    *)
      cp "${source}" "${destination}"
      ;;
  esac
}

checksum_for_archive() {
  local checksums_path="$1"
  local archive_name="$2"
  local line
  line="$(awk -v name="${archive_name}" '$2 == name || $2 == "*" name { print; exit }' "${checksums_path}")"
  if [[ -z "${line}" ]]; then
    fail "checksum entry not found for ${archive_name}"
  fi
  echo "${line}" | awk '{print $1}'
}

print_plan() {
  cat <<PLAN
version: ${VERSION}
target: ${TARGET}
archive: ${ARCHIVE_SOURCE}
checksums: ${CHECKSUMS_SOURCE}
prefix: ${PREFIX}
bin dir: ${BIN_DIR}
share dir: ${SHARE_DIR}
runtime home: ${TENTGENT_HOME:-"(platform default)"}
python bootstrap: ${PYTHON_BOOTSTRAP}
doctor: ${RUN_DOCTOR}
PLAN
}

ARCHIVE_SOURCE=""
CHECKSUMS_SOURCE=""
CHECKSUMS_SOURCE_SET=""
PREFIX="${TENTGENT_INSTALL_PREFIX:-${HOME}/.local}"
TARGET=""
DRY_RUN="false"
PYTHON_BOOTSTRAP="true"
RUN_DOCTOR="true"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --archive)
      ARCHIVE_SOURCE="${2:-}"
      [[ -n "${ARCHIVE_SOURCE}" ]] || fail "--archive requires a path or URL"
      shift 2
      ;;
    --checksums)
      CHECKSUMS_SOURCE="${2:-}"
      [[ -n "${CHECKSUMS_SOURCE}" ]] || fail "--checksums requires a path or URL"
      CHECKSUMS_SOURCE_SET="true"
      shift 2
      ;;
    --version)
      VERSION="${2:-}"
      [[ -n "${VERSION}" ]] || fail "--version requires a value"
      shift 2
      ;;
    --prefix)
      PREFIX="${2:-}"
      [[ -n "${PREFIX}" ]] || fail "--prefix requires a path"
      shift 2
      ;;
    --target)
      TARGET="${2:-}"
      [[ -n "${TARGET}" ]] || fail "--target requires a value"
      shift 2
      ;;
    --dry-run)
      DRY_RUN="true"
      shift
      ;;
    --skip-python-bootstrap)
      PYTHON_BOOTSTRAP="false"
      shift
      ;;
    --skip-doctor)
      RUN_DOCTOR="false"
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

TARGET="${TARGET:-$(host_target)}"
PACKAGE_NAME="tentgent-${VERSION}-${TARGET}"
ARCHIVE_NAME="${PACKAGE_NAME}.tar.gz"
BASE_URL="${TENTGENT_INSTALL_BASE_URL:-${DEFAULT_BASE_URL}/${VERSION}}"
ARCHIVE_SOURCE="${ARCHIVE_SOURCE:-${BASE_URL}/${ARCHIVE_NAME}}"
CHECKSUMS_SOURCE="${CHECKSUMS_SOURCE:-${BASE_URL}/checksums.txt}"
if [[ -z "${CHECKSUMS_SOURCE_SET:-}" && -n "${ARCHIVE_SOURCE:-}" ]]; then
  case "${ARCHIVE_SOURCE}" in
    https://*) ;;
    file://*)
      archive_dir="$(dirname "${ARCHIVE_SOURCE#file://}")"
      if [[ -f "${archive_dir}/checksums.txt" ]]; then
        CHECKSUMS_SOURCE="file://${archive_dir}/checksums.txt"
      fi
      ;;
    *)
      archive_dir="$(dirname "${ARCHIVE_SOURCE}")"
      if [[ -f "${archive_dir}/checksums.txt" ]]; then
        CHECKSUMS_SOURCE="${archive_dir}/checksums.txt"
      fi
      ;;
  esac
fi
BIN_DIR="${PREFIX}/bin"
SHARE_DIR="${PREFIX}/share/tentgent"

if [[ "${DRY_RUN}" == "true" ]]; then
  print_plan
  exit 0
fi

require_command awk
require_command cp
require_command curl
require_command mkdir
require_command tar

TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

ARCHIVE_PATH="${TMP_DIR}/${ARCHIVE_NAME}"
CHECKSUMS_PATH="${TMP_DIR}/checksums.txt"
EXTRACT_DIR="${TMP_DIR}/extract"

print_plan

echo "==> Fetching Tentgent archive"
download_or_copy "${ARCHIVE_SOURCE}" "${ARCHIVE_PATH}"

echo "==> Fetching checksums"
download_or_copy "${CHECKSUMS_SOURCE}" "${CHECKSUMS_PATH}"

EXPECTED_SHA256="$(checksum_for_archive "${CHECKSUMS_PATH}" "${ARCHIVE_NAME}")"
ACTUAL_SHA256="$(checksum_file "${ARCHIVE_PATH}")"
if [[ "${EXPECTED_SHA256}" != "${ACTUAL_SHA256}" ]]; then
  fail "checksum mismatch for ${ARCHIVE_NAME}: expected ${EXPECTED_SHA256}, got ${ACTUAL_SHA256}"
fi

echo "==> Installing Tentgent to ${PREFIX}"
mkdir -p "${EXTRACT_DIR}" "${BIN_DIR}" "${SHARE_DIR}"
tar -xzf "${ARCHIVE_PATH}" -C "${EXTRACT_DIR}"

[[ -x "${EXTRACT_DIR}/bin/tentgent" ]] || fail "archive is missing bin/tentgent"
[[ -d "${EXTRACT_DIR}/share/tentgent/python" ]] || fail "archive is missing share/tentgent/python"
[[ -d "${EXTRACT_DIR}/share/tentgent/scripts" ]] || fail "archive is missing share/tentgent/scripts"

cp "${EXTRACT_DIR}/bin/tentgent" "${BIN_DIR}/tentgent"
rm -rf "${SHARE_DIR}/python" "${SHARE_DIR}/scripts"
cp -R "${EXTRACT_DIR}/share/tentgent/python" "${SHARE_DIR}/python"
cp -R "${EXTRACT_DIR}/share/tentgent/scripts" "${SHARE_DIR}/scripts"
chmod +x "${BIN_DIR}/tentgent"
chmod +x "${SHARE_DIR}/scripts/bootstrap-uv.sh"
chmod +x "${SHARE_DIR}/scripts/bootstrap-python-env.sh"

if [[ "${PYTHON_BOOTSTRAP}" == "true" ]]; then
  echo "==> Bootstrapping managed Python runtime"
  "${SHARE_DIR}/scripts/bootstrap-python-env.sh" --project "${SHARE_DIR}/python"
else
  echo "==> Skipping Python bootstrap"
fi

if [[ "${RUN_DOCTOR}" == "true" ]]; then
  echo "==> Running tentgent doctor"
  "${BIN_DIR}/tentgent" doctor
else
  echo "==> Skipping tentgent doctor"
fi

cat <<NEXT
==> Tentgent installed
binary: ${BIN_DIR}/tentgent

If ${BIN_DIR} is not on PATH, add it before running:
  export PATH="${BIN_DIR}:\$PATH"

Verify later with:
  tentgent doctor
NEXT
