#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: scripts/macos-notarize-package.sh --archive <PATH> --target <TARGET>

Submit a macOS Tentgent package to Apple notarization using App Store Connect
API key credentials from the environment.
USAGE
}

fail() {
  echo "macos-notarize-package: $*" >&2
  exit 1
}

require_command() {
  local name="$1"
  if ! command -v "${name}" >/dev/null 2>&1; then
    fail "required command not found: ${name}"
  fi
}

macos_keychain_access_group() {
  local team_id="$1"
  echo "${team_id}.com.tentserv.tentgent"
}

verify_macos_entitlements() {
  local binary_path="$1"
  local team_id="$2"
  local access_group
  local entitlements
  access_group="$(macos_keychain_access_group "${team_id}")"

  entitlements="$(codesign -d --entitlements - "${binary_path}" 2>/dev/null || true)"
  if [[ "${entitlements}" != *"<key>keychain-access-groups</key>"* ]] ||
    [[ "${entitlements}" != *"<string>${access_group}</string>"* ]] ||
    [[ "${entitlements}" != *"<key>com.apple.developer.team-identifier</key>"* ]] ||
    [[ "${entitlements}" != *"<string>${team_id}</string>"* ]]; then
    printf '%s\n' "${entitlements}" >&2
    fail "signed binary is missing expected Keychain entitlements for ${access_group}"
  fi
}

require_env() {
  local name="$1"
  if [[ -z "${!name:-}" ]]; then
    fail "required environment variable is missing: ${name}"
  fi
}

write_base64_env_to_file() {
  local env_name="$1"
  local output_path="$2"

  ENV_NAME="${env_name}" OUTPUT_PATH="${output_path}" python3 - <<'PY'
import base64
import os
from pathlib import Path

name = os.environ["ENV_NAME"]
output_path = Path(os.environ["OUTPUT_PATH"])
data = base64.b64decode(os.environ[name])
output_path.write_bytes(data)
PY
}

archive_path=""
target=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --archive)
      archive_path="${2:-}"
      [[ -n "${archive_path}" ]] || fail "--archive requires a path"
      shift 2
      ;;
    --target)
      target="${2:-}"
      [[ -n "${target}" ]] || fail "--target requires a target triple"
      shift 2
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      usage
      fail "unknown argument: $1"
      ;;
  esac
done

[[ -n "${archive_path}" ]] || fail "--archive is required"
[[ -n "${target}" ]] || fail "--target is required"
[[ -f "${archive_path}" ]] || fail "archive not found: ${archive_path}"

case "${target}" in
  aarch64-apple-darwin | x86_64-apple-darwin) ;;
  *) fail "notarization target must be a macOS target: ${target}" ;;
esac

if [[ "$(uname -s)" != "Darwin" ]]; then
  fail "notarization must run on macOS"
fi

require_command codesign
require_command ditto
require_command python3
require_command tar
require_command xcrun

require_env APPLE_NOTARY_KEY_BASE64
require_env APPLE_NOTARY_KEY_ID
require_env APPLE_NOTARY_ISSUER_ID
require_env APPLE_TEAM_ID

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "${tmp_dir}"
}
trap cleanup EXIT

extract_dir="${tmp_dir}/extract"
payload_parent="${tmp_dir}/payload"
package_name="$(basename "${archive_path}")"
package_name="${package_name%.tar.gz}"
package_name="${package_name%.zip}"
payload_dir="${payload_parent}/${package_name}"
notary_key_path="${tmp_dir}/AuthKey_${APPLE_NOTARY_KEY_ID}.p8"
notary_archive="${tmp_dir}/${package_name}-notary.zip"

mkdir -p "${extract_dir}" "${payload_parent}"

case "${archive_path}" in
  *.tar.gz)
    tar -xzf "${archive_path}" -C "${extract_dir}"
    ;;
  *.zip)
    ditto -x -k "${archive_path}" "${extract_dir}"
    ;;
  *)
    fail "unsupported macOS archive extension: ${archive_path}"
    ;;
esac

binary_path="${extract_dir}/bin/tentgent"
[[ -x "${binary_path}" ]] || fail "archive is missing executable bin/tentgent"

codesign --verify --strict --verbose=2 "${binary_path}"
codesign_details="$(codesign -dv "${binary_path}" 2>&1)"
if [[ "${codesign_details}" != *"TeamIdentifier=${APPLE_TEAM_ID}"* ]]; then
  echo "${codesign_details}" >&2
  fail "signed binary TeamIdentifier does not match APPLE_TEAM_ID"
fi
verify_macos_entitlements "${binary_path}" "${APPLE_TEAM_ID}"

ditto "${extract_dir}" "${payload_dir}"
ditto -c -k --sequesterRsrc --keepParent "${payload_dir}" "${notary_archive}"

write_base64_env_to_file APPLE_NOTARY_KEY_BASE64 "${notary_key_path}"
chmod 600 "${notary_key_path}"

xcrun notarytool submit "${notary_archive}" \
  --key "${notary_key_path}" \
  --key-id "${APPLE_NOTARY_KEY_ID}" \
  --issuer "${APPLE_NOTARY_ISSUER_ID}" \
  --wait

# Bare CLI executables are not app bundles, so Gatekeeper's spctl exec
# assessment can reject them with "code is valid but does not seem to be an app"
# even after Apple accepts the notarization submission. The CI gate for this
# archive is notarytool acceptance plus strict Developer ID signature validation.
codesign --verify --strict --verbose=2 "${binary_path}"
echo "macOS package notarization accepted for ${archive_path}"
