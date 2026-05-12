#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${ROOT_DIR}/dist"
VERSION="${TENTGENT_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "${ROOT_DIR}/Cargo.toml" | head -n 1)}"

usage() {
  cat <<'USAGE'
Usage: scripts/package-local.sh [OPTIONS]

Build a release-like local Tentgent archive.

Options:
  --print-plan       Print the resolved package plan and exit without building.
  -h, --help         Show this help.

Environment:
  TENTGENT_VERSION  Override the package version.
  TENTGENT_TARGET   Override the target triple used in the artifact name.
USAGE
}

fail() {
  echo "error: $*" >&2
  exit 1
}

uname_target() {
  local os
  local arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "${os}:${arch}" in
    Darwin:arm64) echo "aarch64-apple-darwin" ;;
    Darwin:x86_64) echo "x86_64-apple-darwin" ;;
    Linux:x86_64) echo "x86_64-unknown-linux-gnu" ;;
    MINGW*:x86_64 | MSYS*:x86_64 | CYGWIN*:x86_64) echo "x86_64-pc-windows-msvc" ;;
    *)
      echo "unsupported-${os}-${arch}" >&2
      echo "unsupported"
      ;;
  esac
}

require_command() {
  local name="$1"
  if ! command -v "${name}" >/dev/null 2>&1; then
    fail "required command not found: ${name}"
  fi
}

validate_package_target() {
  local target="$1"
  case "${target}" in
    aarch64-apple-darwin | x86_64-apple-darwin | x86_64-pc-windows-msvc | x86_64-unknown-linux-gnu) ;;
    *) fail "unsupported package target: ${target}" ;;
  esac
}

checksum_command() {
  if command -v shasum >/dev/null 2>&1; then
    echo "shasum -a 256"
    return
  fi
  if command -v sha256sum >/dev/null 2>&1; then
    echo "sha256sum"
    return
  fi
  echo "error: shasum or sha256sum is required" >&2
  exit 1
}

sign_macos_binary() {
  local binary_path="$1"
  if [[ "$(uname -s)" != "Darwin" ]]; then
    return
  fi
  if ! command -v codesign >/dev/null 2>&1; then
    echo "warning: codesign not found; macOS binary will not be ad-hoc signed" >&2
    return
  fi
  codesign --force --sign - "${binary_path}"
}

archive_extension_for_target() {
  local target="$1"
  case "${target}" in
    *windows-msvc) echo "zip" ;;
    *) echo "tar.gz" ;;
  esac
}

binary_name_for_target() {
  local target="$1"
  case "${target}" in
    *windows-msvc) echo "tentgent.exe" ;;
    *) echo "tentgent" ;;
  esac
}

host_matches_target() {
  local target="$1"
  local os
  local arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "${target}:${os}:${arch}" in
    aarch64-apple-darwin:Darwin:arm64) return 0 ;;
    x86_64-apple-darwin:Darwin:x86_64) return 0 ;;
    x86_64-unknown-linux-gnu:Linux:x86_64) return 0 ;;
    x86_64-pc-windows-msvc:MINGW*:x86_64) return 0 ;;
    x86_64-pc-windows-msvc:MSYS*:x86_64) return 0 ;;
    x86_64-pc-windows-msvc:CYGWIN*:x86_64) return 0 ;;
    *) return 1 ;;
  esac
}

windows_path() {
  local path="$1"
  if command -v cygpath >/dev/null 2>&1; then
    cygpath -w "${path}"
  else
    echo "${path}"
  fi
}

print_plan() {
  local target="$1"
  local archive_extension="$2"
  local binary_name="$3"
  local package_name="$4"
  local archive_path="$5"
  local checksums_path="$6"
  local host_match="false"

  if host_matches_target "${target}"; then
    host_match="true"
  fi

  cat <<PLAN
version: ${VERSION}
target: ${target}
archive extension: ${archive_extension}
binary name: ${binary_name}
package: ${package_name}
archive: ${archive_path}
checksums: ${checksums_path}
host matches target: ${host_match}
PLAN
}

create_archive() {
  local target="$1"
  local staging_dir="$2"
  local archive_path="$3"

  case "${target}" in
    *windows-msvc)
      require_command powershell.exe
      local staging_windows
      local archive_windows
      staging_windows="$(windows_path "${staging_dir}")"
      archive_windows="$(windows_path "${archive_path}")"
      powershell.exe -NoProfile -ExecutionPolicy Bypass -Command \
        "Compress-Archive -Path (Join-Path '${staging_windows}' '*') -DestinationPath '${archive_windows}' -Force"
      ;;
    *)
      require_command tar
      tar -C "${staging_dir}" -czf "${archive_path}" \
        bin \
        share \
        README.md \
        LICENSE
      ;;
  esac
}

copy_python_project() {
  local destination="$1"
  mkdir -p "${destination}"

  tar \
    --exclude='.venv' \
    --exclude='__pycache__' \
    --exclude='*.pyc' \
    --exclude='.pytest_cache' \
    --exclude='.ruff_cache' \
    --exclude='.mypy_cache' \
    --exclude='.DS_Store' \
    -C "${ROOT_DIR}/python/tentgent-daemon" \
    -cf - . | tar -C "${destination}" -xf -
}

main() {
  local print_plan_only="false"

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --print-plan)
        print_plan_only="true"
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

  local target
  local archive_extension
  local binary_name
  local package_name
  local staging_dir
  local archive_path
  local checksums_path

  target="${TENTGENT_TARGET:-$(uname_target)}"
  validate_package_target "${target}"
  archive_extension="$(archive_extension_for_target "${target}")"
  binary_name="$(binary_name_for_target "${target}")"
  package_name="tentgent-${VERSION}-${target}"
  staging_dir="${DIST_DIR}/.staging/${package_name}"
  archive_path="${DIST_DIR}/${package_name}.${archive_extension}"
  checksums_path="${DIST_DIR}/checksums.txt"

  if [[ "${print_plan_only}" == "true" ]]; then
    print_plan "${target}" "${archive_extension}" "${binary_name}" "${package_name}" "${archive_path}" "${checksums_path}"
    exit 0
  fi

  if ! host_matches_target "${target}"; then
    fail "${target} packaging must run on a native matching host"
  fi

  require_command cargo

  echo "==> Building Tentgent release binary"
  cargo build --release --bin tentgent

  echo "==> Preparing local package ${package_name}"
  rm -rf "${staging_dir}"
  mkdir -p \
    "${staging_dir}/bin" \
    "${staging_dir}/share/tentgent/python" \
    "${staging_dir}/share/tentgent/scripts"

  cp "${ROOT_DIR}/target/release/${binary_name}" "${staging_dir}/bin/${binary_name}"
  case "${target}" in
    *apple-darwin) sign_macos_binary "${staging_dir}/bin/${binary_name}" ;;
  esac
  cp "${ROOT_DIR}/README.md" "${staging_dir}/README.md"
  cp "${ROOT_DIR}/LICENSE" "${staging_dir}/LICENSE"
  cp "${ROOT_DIR}/scripts/bootstrap-uv.sh" "${staging_dir}/share/tentgent/scripts/bootstrap-uv.sh"
  cp "${ROOT_DIR}/scripts/bootstrap-python-env.sh" "${staging_dir}/share/tentgent/scripts/bootstrap-python-env.sh"
  cp "${ROOT_DIR}/scripts/install.sh" "${staging_dir}/share/tentgent/scripts/install.sh"
  cp "${ROOT_DIR}/scripts/install.ps1" "${staging_dir}/share/tentgent/scripts/install.ps1"
  copy_python_project "${staging_dir}/share/tentgent/python"

  echo "==> Creating ${archive_path}"
  mkdir -p "${DIST_DIR}"
  rm -f "${archive_path}"
  create_archive "${target}" "${staging_dir}" "${archive_path}"

  echo "==> Writing ${checksums_path}"
  (
    cd "${DIST_DIR}"
    checksum_command | xargs -I {} sh -c '{} "$1"' sh "$(basename "${archive_path}")"
  ) >"${checksums_path}"

  echo "==> Package complete"
  echo "archive: ${archive_path}"
  echo "checksums: ${checksums_path}"
}

main "$@"
